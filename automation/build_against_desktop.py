#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Run Firefox desktop / HNT (Home & New Tab) tests against this application-services working tree.
# https://mozilla.github.io/application-services/book/howtos/vendoring-into-mozilla-central.html
#
# Requirements:
# - python
# - application-services built and working.
# - a `firefox`/`mozilla-central` repository set up and working to use.
#               - See: https://firefox-source-docs.mozilla.org/contributing/contribution_quickref.html
# Example: ./automation/build_against_desktop.py --action build-without-testing --firefox-dir ../firefox --verbose --as-commit [COMMIT-HASH]
# and arg to clean up only?
# Arguments:
#       --action            => Can be either `run-tests` (default) or `build-without-testing`, or `run` (which runs it locally with `./mach run`)
#       --firefox-dir       => Working mozilla-central directory
#                              https://firefox-source-docs.mozilla.org/contributing/contribution_quickref.html
#       --as-commit         => Commit to the `application-services` repository.
#       --mozconfig         => Absolute path to the mozconfig file to be used.
#       --verbose           => Includes the stdout of subprocesses (like the xcodebuild output, or other bootstrapping scripts)
#       --clean-up          => Whether to perform the on-success cleanup step at the end of a successful build (default is True). This clean-up step happens either way on an error or graceful exit (such as with `--action run`).
#       --test-name         => Test name to run with `./mach test`. If `run-tests` is attached, but no `--test-name` is provided, the default command will be `./mach test --auto` where appropriate tests will be guessed.
#       --ignore-modified   => Whether to run the vendoring step with `--ignore-modified` (eg: to allow running the command multiple times, vendoring multiple times, etc.) 
import argparse
import subprocess
import os
import tempfile
from pathlib import Path
from shared import (
    find_app_services_root,
    step_msg,
    err_msg,
    run_cmd_is_successful,
    dir_file_sanity_check,
)

DEFAULT_MOZ_CONFIG_LOCATION = "mozconfig_desktop"
DEFAULT_MOZ_CONFIG = """
ac_add_options --enable-project=browser
"""
MOZILLA_FF_GRADLE_PROPERTIES_PATH = "gradle.properties"
COMPONENTS_FOLDER_AS_SUBPATH = "components"
COMPONENTS_FOLDER_MC_SUBPATH = "third_party/application-services/components"

def build_against_desktop(
    firefox_dir,
    as_commit,
    moz_config_location,
    test_name,
    ignore_modified,
    verbose,
    action,
):
    subprocess_stdout = None if verbose else subprocess.DEVNULL
    subprocess_stderr = None if verbose else subprocess.DEVNULL

    if action is None:
        action = "run-tests"

    firefox_repo_path = Path(firefox_dir)
    tmp_dir_path = Path(tempfile.mkdtemp(suffix="-test-desktop"))

    app_services_path = find_app_services_root()

    step_msg("Checking for sanity of application-services repository...")
    if not dir_file_sanity_check(
        app_services_path, "application-services", ["megazords", "components"]
    ):
        return False

    # MOZCONFIG handling.
    # Idea here is that mozconfig settings (primary indicator of how firefox is built) can't be passed
    # without `configure`, which is not recommended. However, we can pass test fixture mozconfig files themselves as env variables.
    if moz_config_location is None:
        moz_config_location = os.path.abspath(
            tmp_dir_path / DEFAULT_MOZ_CONFIG_LOCATION
        )
        with open(moz_config_location, "w") as file:
            file.write(DEFAULT_MOZ_CONFIG)

    if not os.path.isabs(moz_config_location):
        err_msg(
            f"`mozconfig` path passed: `{moz_config_location}` must be an absolute path."
        )
        return False
    if not os.path.isfile(moz_config_location):
        err_msg(f"`mozconfig` path passed: `{moz_config_location}` could not be found.")
        return False
    step_msg(f"Using `mozconfig` path: `{moz_config_location}`. Displaying:")
    with open(moz_config_location) as f:
        print(f.read())

    # Basic sanity check here. Not remotely exhaustive, just to make sure the wrong directory wasn't passed.
    step_msg("Checking for sanity of firefox repository...")
    if not dir_file_sanity_check(
        firefox_repo_path,
        "mozilla-central",
        ["mach", "CLOBBER", "gradlew", "Cargo.toml", "local.properties"],
    ):
        return False

    # Environment verification check
    step_msg("Verifying Desktop environment...")
    if not run_cmd_is_successful(
        "./libs/verify-desktop-environment.sh",
        cwd=app_services_path,
        shell=True,
        stdout=subprocess_stdout,
        stderr=subprocess_stderr,
    ):
        err_msg(
            "Failed to run `./libs/verify-android-environment.sh` in app-services environment. Run this script and follow any instructions given until it succeeds, then try again."
        )
        return False

    # The vendoring step
    step_msg("Vendoring commit: `{as_commit}`...")
    ignore_modified_str = "--ignore-modified" if ignore_modified else ""
    if not run_cmd_is_successful(
        f"./mach vendor third_party/application-services/moz.yaml --force {ignore_modified_str} -r {as_commit}",
        cwd=firefox_repo_path,
        shell=True,
        stdout=subprocess_stdout,
    ):
        err_msg("Failed to vendor commit `{as_commit} with `./mach vendor third_party/application-services/moz.yaml --force -r {as_commit}`")
        err_msg("If this is because of uncommitted changes, either revert the vendor or pass `--ignore-modified`.")
        return False

    step_msg("Updating vendored dependencies rust...")
    if not run_cmd_is_successful(
        "./mach vendor rust --ignore-modified",
        cwd=firefox_repo_path,
        shell=True,
        stdout=subprocess_stdout,
    ):
        err_msg("Failed to vendor dependencies with `./mach vendor rust`")
        return False

    # We are pointing to a new area as if we vendored, so we regenerate.
    step_msg("Regenerating uniffi bindings (mozconfig=`{moz_config_location}`)...")
    if not run_cmd_is_successful(
        "./mach uniffi generate",
        cwd=firefox_repo_path,
        shell=True,
        stdout=subprocess_stdout,
    ):
        err_msg("Failed to generate uniffi bindings with: `./mach uniffi generate`/")
        return False

    step_msg(
        f"Compiling firefox with `./mach build` (mozconfig=`{moz_config_location}`)..."
    )
    if not run_cmd_is_successful(
        f"MOZCONFIG={moz_config_location} ./mach build",
        cwd=firefox_repo_path,
        shell=True,
        stdout=subprocess_stdout,
    ):
        err_msg("Failed to compile firefox with `./mach build`.")
        return False

    if action == "run-tests":
        step_msg(
            f"Compiling firefox with mozconfig with `./mach test` (mozconfig=`{moz_config_location}`)..."
        )
        test_string = test_name if test_name is not None else "--auto"
        step_msg(f"Running test command `./mach test {test_string}`")
        if not run_cmd_is_successful(
            f"MOZCONFIG={moz_config_location} ./mach test {test_string}",
            cwd=firefox_repo_path,
            shell=True,
            stdout=subprocess_stdout,
        ):
            err_msg(f"Failed to run tests against firefox with ./mach test {test_string}.")
            return False
    elif action == "run":
        step_msg(
            f"Running firefox with mozconfig with `./mach run` (mozconfig=`{moz_config_location}`)..."
        )
        if not run_cmd_is_successful(
            f"MOZCONFIG={moz_config_location} ./mach run",
            cwd=firefox_repo_path,
            shell=True,
            stdout=subprocess_stdout,
        ):
            err_msg("Failed to run tests against firefox with ./mach run.")
            return False

    step_msg("Successfully built against HNT!")
    return True


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run Firefox HNT tests against this application-services working tree."
    )

    parser.add_argument(
        "--verbose",
        help="Display subprocess logs for compilation processes (off by default).",
        action=argparse.BooleanOptionalAction,
    )
    parser.add_argument(
        "--action",
        choices=["run", "run-tests", "build-without-testing"],
        help="Whether to run tests after the build step is complete..",
    )
    parser.add_argument(
        "--firefox-dir",
        required=True,
        help="Path to existing bootstrapped `mozilla-central` directory.",
    )
    parser.add_argument(
        "--as-commit",
        required=True,
        help="`application-services` commit to vendor into firefox.",
    )
    parser.add_argument(
        "--mozconfig",
        help="Absolute path to the desired mozconfig file. This affects the build destination, ensure it specifies android if you override it.",
    )
    parser.add_argument(
        "--test-name",
        help="Name of the test file to run, as if you were running `./mach test ARG`.",
    )

    parser.add_argument(
        "--ignore-modified",
        help="Whether to run the vendoring step with `--ignore-modified` (eg: to allow running the command multiple times, vendoring multiple times, etc.)",
        action=argparse.BooleanOptionalAction,
        default=True,
    )

    args = parser.parse_args()
    firefox_dir = args.firefox_dir
    as_commit = args.as_commit
    verbose = args.verbose
    moz_config_location = args.mozconfig
    action = args.action
    test_name = args.test_name
    ignore_modified = args.ignore_modified
    build_against_desktop(firefox_dir, as_commit, moz_config_location, test_name, ignore_modified, verbose, action)
