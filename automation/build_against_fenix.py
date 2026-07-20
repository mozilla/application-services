#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Run Firefox fenix tests against this application-services working tree.
# https://github.com/mozilla/application-services/blob/main/docs/howtos/locally-published-components-in-fenix.md
# So, for now, we need to use an existing respository.
#
# Requirements:
# - python
# - application-services built and working.
# - a `firefox`/`mozilla-central` repository set up and working to use.
#               - See: https://firefox-source-docs.mozilla.org/contributing/contribution_quickref.html
#
# Usage: ./automation/build_against_fenix.py --action build-without-testing --firefox-dir ../firefox --prefix-ff fenix --prefix-as ads-client --verbose
#
# Arguments:
#       --action            => Can be either `run-tests` (default) or `build-without-testing`
#       --firefox-dir       => Working mozilla-central directory
#                              https://firefox-source-docs.mozilla.org/contributing/contribution_quickref.html
#       --mozconfig         => Absolute path to the mozconfig file to be used.
#       --prefix-ff         => Prefix to be used in gradle commands to firefox repo. eg: `./gradlew fenix:assembleDebug`. For example: `geckoview`, `fenix`, `focus`."
#       --prefix-as         => Crate prefix to be used in gradle commands to application-services repo. eg: `./gradlew ads-client:assembleDebug`. For example: `ads-client`, `fxaclient`."
#       --verbose           => Includes the stdout of subprocesses (like the xcodebuild output, or other bootstrapping scripts)
#       --clear-bindings    => Whether or not to clear existing bindings and cached artifacts, such as with "./gradlew fenix:clean"
import argparse
import subprocess
import os
import tempfile
from pathlib import Path
import re
from shared import (
    find_app_services_root,
    set_gradle_substitution_path,
    step_msg,
    err_msg,
    run_cmd_is_successful,
    dir_file_sanity_check,
)

DEFAULT_MOZ_CONFIG_LOCATION = "mozconfig_android"
DEFAULT_MOZ_CONFIG = """
ac_add_options --enable-project=mobile/android
"""
MOZILLA_FF_GRADLE_PROPERTIES_PATH = "gradle.properties"


# Replaces org.gradle.configuration-cache=true with a commented version.
def comment_gradle_cache_line(firefox_repo_path):
    """
    Comments out the gradle cache line pursuant to step 2 here.
    https://github.com/mozilla/application-services/blob/main/docs/howtos/locally-published-components-in-fenix.md#pre-requisites
    """

    properties_file_path = Path(firefox_repo_path) / MOZILLA_FF_GRADLE_PROPERTIES_PATH
    if not os.path.isfile(properties_file_path):
        err_msg(
            "Could not find an instance of `gradle.properties` to modify. Please ensure the `m-c`/`firefox` directory is lined up correctly."
        )
        return False

    # Uses regex. Matches the binaryTarget listed and replaces it with the following string.
    replace_with = """# org.gradle.configuration-cache=true"""
    step_msg(f"Writing to gradle.properties:\n{replace_with}")
    regex = re.compile(r"^#*\s*org\.gradle\.configuration-cache=true\s*$", re.MULTILINE)

    with open(properties_file_path, "r+") as f:
        data = f.read()
        # Regex string matches this tidbit.
        package_file = regex.sub(replace_with, data)
        f.seek(0)
        f.write(package_file)
        f.truncate()

    return True


def build_against_fenix(
    firefox_dir,
    moz_config_location,
    prefix_ff,
    prefix_as,
    clear_bindings,
    verbose,
    action,
):
    subprocess_stdout = None if verbose else subprocess.DEVNULL
    subprocess_stderr = None if verbose else subprocess.DEVNULL

    if action is None:
        action = "run-tests"

    firefox_repo_path = Path(firefox_dir)
    tmp_dir_path = Path(tempfile.mkdtemp(suffix="-test-fenix"))

    app_services_path = find_app_services_root()

    step_msg("Checking for sanity of application-services repository...")
    if not dir_file_sanity_check(
        app_services_path, "application-services", ["megazords", "components"]
    ):
        return False

    prefix_as_string = f"{prefix_as}:" if prefix_as else ""
    prefix_ff_string = f"{prefix_ff}:" if prefix_ff else ""

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

    # The following steps modify several property files in the m-c repository provided:
    # Key step: gradle can use a different application-services directory
    step_msg(f"Configuring {firefox_repo_path} to autopublish appservices")
    if not set_gradle_substitution_path(
        firefox_repo_path,
        "autoPublish.application-services.dir",
        find_app_services_root(),
    ):
        err_msg(
            "Failed in attempting to set `local.properties` `autoPublish.application-services.dir`"
        )
        return False

    # Comments out gradle cache in local.properties
    step_msg(f"Configuring {firefox_repo_path} to disable gradle configuration-cache")
    if not comment_gradle_cache_line(firefox_repo_path):
        err_msg(
            "Failed in attempting to set `gradle.properties` `#org.gradle.configuration-cache=true`"
        )
        return False

    # Environment verification check
    step_msg("Verifying Android environment...")
    if not run_cmd_is_successful(
        "./libs/verify-android-environment.sh",
        cwd=app_services_path,
        shell=True,
        stdout=subprocess_stdout,
        stderr=subprocess_stderr,
    ):
        err_msg(
            "Failed to run `./libs/verify-android-environment.sh` in app-services environment. Run this script and follow any instructions given until it succeeds, then try again."
        )
        return False

    # Gradle clean cached files
    if clear_bindings:
        step_msg(
            "Cleaning application-services with gradle to clear cached android bindings..."
        )
        if not run_cmd_is_successful(
            f"./gradlew {prefix_as_string}clean",
            cwd=app_services_path,
            shell=True,
            stdout=subprocess_stdout,
        ):
            err_msg(
                "Could not run ./gradlew clean. Please check to ensure the mozilla-center folder structure is sound."
            )
            return False

    # Run gradle compilations and tests
    step_msg("Compiling application-services with gradle to test android bindings...")
    if not run_cmd_is_successful(
        f"./gradlew {prefix_as_string}assembleDebug",
        cwd=app_services_path,
        shell=True,
        stdout=subprocess_stdout,
    ):
        err_msg("Failed to compile application-services with gradle.")
        return False

    step_msg(
        f"Compiling firefox with mozconfig with `./gradlew {prefix_ff_string}assembleDebug` (mozconfig=`{moz_config_location}`)..."
    )
    if not run_cmd_is_successful(
        f"MOZCONFIG={moz_config_location} ./gradlew {prefix_ff_string}assembleDebug",
        cwd=firefox_repo_path,
        shell=True,
        stdout=subprocess_stdout,
    ):
        err_msg("Failed to compile firefox with gradle.")
        return False

    if action == "run-tests":
        step_msg(
            f"Compiling firefox with mozconfig with `./gradlew {prefix_ff_string}testDebug` (mozconfig=`{moz_config_location}`)..."
        )
        if not run_cmd_is_successful(
            f"MOZCONFIG={moz_config_location} ./gradlew {prefix_ff_string}testDebug",
            cwd=firefox_repo_path,
            shell=True,
            stdout=subprocess_stdout,
        ):
            err_msg("Failed to run tests against firefox with gradle.")
            return False
    step_msg("Successfully built against Android!")
    return True


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run Firefox Android tests against this application-services working tree."
    )

    parser.add_argument(
        "--verbose",
        help="Display subprocess logs for compilation processes (off by default).",
        action=argparse.BooleanOptionalAction,
    )
    parser.add_argument(
        "--action",
        choices=["run-tests", "build-without-testing"],
        help="Whether to run tests after the build step is complete..",
    )
    parser.add_argument(
        "--firefox-dir",
        required=True,
        help="Path to existing bootstrapped `mozilla-central` directory.",
    )
    parser.add_argument(
        "--mozconfig",
        help="Absolute path to the desired mozconfig file. This affects the build destination, ensure it specifies android if you override it.",
    )
    parser.add_argument(
        "--prefix-ff",
        help="Prefix name to pass to mozilla-central compilation to reduce the amount needing to build or test. For example: `geckoview`, `fenix`, `focus`.",
    )
    parser.add_argument(
        "--prefix-as",
        help="Crate name to pass for preliminary application-services android building step. For example: `ads-client`, `fxaclient`",
    )
    parser.add_argument(
        "--clear-previous-bindings",
        help="Clear existing uniffi binding files from the firefox android build folder. (`/gradlew clean`). This shares any prefixes supplied by `--prefix-as`. If unrelated files need to be cleared, do not pass a --prefix-as argument",
        action=argparse.BooleanOptionalAction,
    )

    args = parser.parse_args()
    firefox_dir = args.firefox_dir
    verbose = args.verbose
    moz_config_location = args.mozconfig
    action = args.action
    prefix_ff = args.prefix_ff
    prefix_as = args.prefix_as
    clear_bindings = args.clear_previous_bindings
    build_against_fenix(
        firefox_dir,
        moz_config_location,
        prefix_ff,
        prefix_as,
        clear_bindings,
        verbose,
        action,
    )
