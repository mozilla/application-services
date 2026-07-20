#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Run Firefox-iOS tests against this application-services working tree.
# https://github.com/mozilla/application-services/blob/main/docs/howtos/locally-published-components-in-firefox-ios.md
# Requirements: 
# - python
# - application-services built and working.
# - xcpretty (`gem install xcpretty`)
# - xcode + xcodebuild + xcodetools setup and running (a successful build of the firefox-ios repository)
#
# Usage: ./automation/build_against_ios.py
# Arguments:
#       --action                    => Can be either `run-tests` (default) or `build-without-testing`
#       --remote-repo-url           => Fetch firefox-ios repository from this URL instead. Exclusive with `use-local-repo`
#       --use-local-repo            => Use a local firefox-ios repository instead (at the provided path). Exclusive with `remote-repo-url`.
#       --verbose                   => Includes the stdout of subprocesses (like the xcodebuild output, or other bootstrapping scripts)
#       --clear-previous-bindings   => Clear existing uniffi binding swift files from both the iOS and A-S generated folders. Use if files were created that need to be cleared (eg: a file of a name that is no longer used).
#       --clean-ios-caches          => Runs the code equivalent of Xcode's 'Clean Build Folder' 
# 
import argparse
import subprocess
import os
import tempfile
from pathlib import Path
import shutil
import glob
from shared import fatal_err, find_app_services_root, run_cmd_checked, step_msg, err_msg, run_cmd_is_successful

DEFAULT_REMOTE_REPO_URL = "https://github.com/mozilla-mobile/firefox-ios.git"

def build_against_ios(local_ios_repo_path, remote_repo_url, scheme, test_plan, clear_previous_bindings, clean_ios_caches, verbose, action):

    subprocess_stdout = None
    if not verbose:
        subprocess_stdout = subprocess.DEVNULL
    subprocess_stderr = None
    if not verbose:
        subprocess_stderr = subprocess.DEVNULL

    if action is None:
        action = "run-tests"

    ios_repo_path = local_ios_repo_path
    app_services_path = find_app_services_root()

    # Basic sanity check here. Not remotely exhaustive, just to make sure some very wrong directory wasn't passed.
    # TODO: modularize
    step_msg("Checking for sanity of application-services repository...")
    for example_file in [
        "megazords", "components"
    ]:
        new_path = app_services_path / example_file
        if not os.path.isfile(new_path) and not os.path.isdir(new_path):
            err_msg(f"`{example_file}` is missing in root of `application-services` directory. Please confirm this is a valid local copy of `application-services` at: {app_services_path}")
            return False



    step_msg("Checking for existence of xcodebuild...")
    if not run_cmd_is_successful("xcodebuild -version", cwd=ios_repo_path, shell=True):
        err_msg("xcodebuild is required to compile application-services for iOS. Please clone the firefox-ios repository and follow the instructions therein.")
        return False

    # Creating temp directory and cloning repository
    step_msg(f"Building application-services against iOS with action: `{action}`")
    if local_ios_repo_path is None:
        ios_repo_path = tempfile.mkdtemp(suffix="-test-ios")
        if remote_repo_url is None:
            remote_repo_url = DEFAULT_REMOTE_REPO_URL
        step_msg(f"Cloning {remote_repo_url}")
        run_cmd_checked(["git", "clone", remote_repo_url, ios_repo_path])

    ios_generated_uniffi_files_path = f"{ios_repo_path}/MozillaRustComponents/Sources/MozillaRustComponentsWrapper/Generated"
    local_repo_generated_uniffi_files_path = f"{app_services_path}/megazords/ios-rust/Sources/MozillaRustComponentsWrapper/Generated"

    # Bootstrapping the iOS repository
    step_msg("Running the firefox-ios bootstrap script...")
    if not run_cmd_is_successful("./bootstrap.sh", 
        cwd=ios_repo_path,
        shell=True,
        stdout=subprocess_stdout
    ):
        err_msg("Failed to bootstrap firefox-ios repository. Please clone the firefox-ios repository and follow the instructions therein.")
        return False
    
    # Verification check
    step_msg("Verifying iOS environment...")
    if not run_cmd_is_successful(
        "./libs/verify-ios-environment.sh",
        cwd=app_services_path,
        shell=True,
        stdout=subprocess_stdout,
        stderr=subprocess_stderr
    ):
            err_msg("Failed to verify environment for iOS. Please run `./libs/verify-ios-environment.sh`, making suggested changes until it succeeds.")
            return False
    
    # Uniffi sanity check
    if not os.path.isdir(ios_generated_uniffi_files_path):
        err_msg(f"Expected path `{ios_generated_uniffi_files_path}` in firefox-ios is missing. Please confirm the repository structure or try cloning it again.")
        return False

    if clear_previous_bindings:
        step_msg("'clear-previous-bindings' is set, clearing uniffi folders")
        
        # Clear the uniffi bindings in the A-S repository as extra files created are not deleted
        # Not relevant to tmp repository
        if os.path.isdir(local_repo_generated_uniffi_files_path):
            for p in Path(local_repo_generated_uniffi_files_path).glob("*.swift"):
                p.unlink()

        # Clear equivalents from the ios repository
        # (Relevant if we are using an existing directory)
        if not run_cmd_is_successful(["git", "checkout", "."], cwd=ios_generated_uniffi_files_path):
            fatal_err("Found an error running git commands to clear previous uniffi folders. Exiting.")
        if not run_cmd_is_successful(["git", "clean", "-f"], cwd=ios_generated_uniffi_files_path):
            fatal_err("Found an error running git commands to clear previous uniffi folders. Exiting.")

    # Build artifacts
    # Unfortunately build_ios_artifacts is writing to stderr, so we need to hide it when it's not verbose.
    step_msg("Building application-services iOS artifacts...")
    if not run_cmd_is_successful(
        "./automation/build_ios_artifacts.sh",
        cwd=app_services_path,
        shell=True,
        check=True,
        stdout=subprocess_stdout,
        stderr=subprocess_stderr
    ):
        err_msg("Failed to build ios artifacts. Please ensure the code compiles and try running `./automation/build_ios_artifacts.sh` from the folder.")
        return False

    if not os.path.isdir(local_repo_generated_uniffi_files_path):
        err_msg(f"Expected path `{local_repo_generated_uniffi_files_path}` in application-services is missing after building. Please confirm the repository structure or try cloning it again.")
        return False


    step_msg("Copying uniffi bindings from:")
    step_msg(f"{local_repo_generated_uniffi_files_path} -> {ios_generated_uniffi_files_path}")
    for file in glob.glob('*.swift', root_dir=local_repo_generated_uniffi_files_path):
        shutil.copy(
            f"{local_repo_generated_uniffi_files_path}/{file}",
            ios_generated_uniffi_files_path
        )

    # Remove the glean_sym file.
    # https://github.com/mozilla/application-services/blob/main/docs/howtos/locally-published-components-in-firefox-ios.md
    step_msg(f"Removing: {ios_generated_uniffi_files_path}/glean_sym.swift")
    os.remove(f"{ios_generated_uniffi_files_path}/glean_sym.swift")

    scheme = "Fennec" if scheme is None else scheme
    test_plan = "Smoketest" if test_plan is None else test_plan
    if clean_ios_caches:
        # TODO: "Reset package caches" part not done yet

        # Clean build folder
        step_msg("Cleaning build folder...")
        if not run_cmd_is_successful(
            f"""\
        set -o pipefail && \
        xcodebuild \
        -workspace ./firefox-ios/Client.xcodeproj/project.xcworkspace \
        -scheme {scheme} \
        clean | \
        xcpretty
        """,
            cwd=ios_repo_path,
            shell=True,
            stdout=subprocess_stdout
        ):
            err_msg("Failed to clean and compile tests on iOS",)
            return False

    # Run the build action
    if action == "build-without-testing":
        step_msg("Running xcodebuild without testing (this may take a few minutes)...")
        if not run_cmd_is_successful(
            f"""\
        set -o pipefail && \
        xcodebuild \
        -workspace ./firefox-ios/Client.xcodeproj/project.xcworkspace \
        -scheme {scheme} \
        -destination 'platform=iOS Simulator,name=iPhone 17' \
        build-for-testing | \
        xcpretty
        """,
            cwd=ios_repo_path,
            shell=True,
            stdout=subprocess_stdout
        ):
            err_msg("Failed to compile and run tests on iOS",)
            return False

    elif action == "run-tests":
        step_msg("Building firefox-ios and running tests (this may take a few minutes)...")
        if not run_cmd_is_successful(
            f"""\
        set -o pipefail && \
        xcodebuild \
        -workspace ./firefox-ios/Client.xcodeproj/project.xcworkspace \
        -scheme {scheme} \
        -destination 'platform=iOS Simulator,name=iPhone 17' \
        -testPlan {test_plan} \
        test | \
        xcpretty
        """,
            cwd=ios_repo_path,
            shell=True,
            stdout=subprocess_stdout
        ):
            err_msg("Failed to compile and run tests on iOS")
            return False

    else:
        err_msg("You must either run `--action run-tests` or `--action build-without-testing` ")
        return False

    step_msg("Successfully built against iOS!")
    return True


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run Firefox-iOS tests against this application-services working tree."
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--use-local-repo",
        metavar="LOCAL_IOS_REPO_PATH",
        help="Use a local copy of firefox-ios instead of cloning it.",
    )
    group.add_argument(
        "--remote-repo-url",
        metavar="REMOTE_REPO_PATH",
        help="Clone a different firefox-ios repository.",
    )

    parser.add_argument("--verbose", help="Includes subprocess running logs.", action=argparse.BooleanOptionalAction)
    parser.add_argument('--clear-previous-bindings', 
                        help="Clear existing uniffi binding swift files from both the iOS and A-S generated folders. Use if files were created that need to be cleared (eg: a file of a name that is no longer used).",
                        action=argparse.BooleanOptionalAction)

    parser.add_argument('--clean-ios-caches', 
                        help="Run Xcode 'Clean Build Folder'",
                        action=argparse.BooleanOptionalAction)

    parser.add_argument(
        "--scheme",
        help="The scheme to run. Likely: `Fennec` (default) or `Firefox`",
        default="Fennec"
    )

    parser.add_argument(
        "--test-plan",
        help="The test plan to test with. Likely: `Smoketest` (default) or `FullFunctionalTestPlan`",
        default="Smoketest"
    )

    parser.add_argument(
        "--action",
        choices=["run-tests", "build-without-testing"],
        help="Run the following action once firefox-ios is set up.",
    )

    args = parser.parse_args()
    local_ios_repo_path = args.use_local_repo
    remote_repo_url = args.remote_repo_url
    clear_previous_bindings = args.clear_previous_bindings
    clean_ios_caches = args.clean_ios_caches
    scheme = args.scheme
    test_plan = args.test_plan
    verbose = args.verbose
    action = args.action

    build_against_ios(local_ios_repo_path, remote_repo_url, scheme, test_plan, clear_previous_bindings, clean_ios_caches, verbose, action)