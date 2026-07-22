#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Run various smoke tests against this application-services working tree.
# Requirements:
# - python
# - application-services built and working.
# For the mac builds:
# - xcpretty (`gem install xcpretty`)
# - xcode + xcodebuild + xcodetools setup and running (a successful build of the firefox-ios repository)
# Arguments:
#       --action                     => Can be either `run-tests` (default) or `build-without-testing`
#       --use_local_firefox_ios      => Use a local firefox-ios repository instead (at the provided path).
#       --verbose                    => Includes the stdout of subprocesses (like the xcodebuild output, or other bootstrapping scripts)
#       --allow-clears               => Clear existing uniffi bindings, swift files, and so on during the various build processes.
#       --use-local-firefox-ios      => Use a local copy of firefox-ios instead of cloning it for iOS tests. Exclusive with `remote-ios-repo-url`
#       --remote-ios-repo-url       => Clone a different firefox-ios repository for iOS tests. Exclusive with `use-local-firefox-ios`
#       --ios-scheme                => The scheme to run for iOS tests. Likely: `Fennec` (default) or `Firefox`
#       --ios-test-plan             => The test plan to test with for iOS tests. Likely: `Smoketest` (default) or `FullFunctionalTestPlan`
import argparse
import time
from shared import err_msg, step_msg
from build_against_hnt import build_against_hnt
from build_against_fenix import build_against_fenix
from build_against_ios import build_against_ios

parser = argparse.ArgumentParser(
    description="Run groups of tests against this application-services working tree."
)

group = parser.add_mutually_exclusive_group()
parser.add_argument(
    "--firefox-dir",
    required=True,
    help="Path to existing bootstrapped `mozilla-central` directory.",
)
parser.add_argument(
    "--verbose",
    help="Display subprocess logs for compilation processes (off by default).",
    action=argparse.BooleanOptionalAction,
)
parser.add_argument(
    "--allow-clears",
    help="Clear existing uniffi bindings, swift files, and so on during the various build processes (what gets cleared varies per platform test).",
    action=argparse.BooleanOptionalAction,
)
parser.add_argument(
    "--action",
    required=True,
    choices=["run-tests", "build-without-testing"],
    help="Run the following action for target's test",
)

# iOS arguments to pass down
group = parser.add_mutually_exclusive_group()
group.add_argument(
    "--use-local-firefox-ios",
    metavar="LOCAL_IOS_REPO_PATH",
    help="Use a local copy of firefox-ios instead of cloning it for iOS tests. Exclusive with `remote-ios-repo-url`",
)
group.add_argument(
    "--remote-ios-repo-url",
    metavar="REMOTE_REPO_PATH",
    help="Clone a different firefox-ios repository for iOS tests. Exclusive with `use-local-firefox-ios`",
)
parser.add_argument(
    "--ios-scheme",
    help="The scheme to run for iOS tests. Likely: `Fennec` (default) or `Firefox`",
    default="Fennec",
)
parser.add_argument(
    "--ios-test-plan",
    help="The test plan to test with for iOS tests. Likely: `Smoketest` (default) or `FullFunctionalTestPlan`",
    default="Smoketest",
)

# Fenix arguments to pass down
parser.add_argument(
    "--prefix-ff",
    help="Prefix name to pass to mozilla-central gradlew compilation to reduce the amount needing to build or test. For example: `geckoview`, `fenix`, `focus`.",
    default="fenix",
)


args = parser.parse_args()
firefox_dir = args.firefox_dir
verbose = args.verbose if args.verbose else False
allow_clears = args.allow_clears
action = args.action

local_firefox_ios = args.use_local_firefox_ios
remote_ios_repo_url = args.remote_ios_repo_url
ios_scheme = args.ios_scheme
ios_test_plan = args.ios_test_plan

prefix_ff = args.prefix_ff

# Build against iOS
start_time_ios = time.time()
success_ios = build_against_ios(
    local_firefox_ios,
    remote_ios_repo_url,
    ios_scheme,
    ios_test_plan,
    clear_previous_bindings=allow_clears,
    clean_ios_caches=allow_clears,
    verbose=verbose,
    action=action,
)
time_diff_ios = time.time() - start_time_ios

# Build against Fenix
start_time_fenix = time.time()
success_fenix = build_against_fenix(
    firefox_dir,
    None,
    prefix_ff,
    prefix_as=None,
    clear_bindings=allow_clears,
    verbose=verbose,
    action=action,
)
time_diff_fenix = time.time() - start_time_fenix

# Build against Desktop
start_time_hnt = time.time()
success_hnt = build_against_hnt(firefox_dir, None, True, verbose=verbose, action=action)
time_diff_hnt = time.time() - start_time_hnt

did_tests_string = "" if action != "run-tests" else " (and tested)"
do_tests_string = "" if action != "run-tests" else " (and test)"
step_msg("Finished building. Results:")
if success_ios:
    step_msg(
        f"Successfully built{did_tests_string} against iOS (elapsed {time_diff_ios:.2f}s)"
    )
else:
    err_msg(
        f"Failed to build{do_tests_string} against iOS (elapsed {time_diff_ios:.2f}s)"
    )
if success_fenix:
    step_msg(
        f"Successfully built{did_tests_string} against Fenix (elapsed {time_diff_fenix:.2f}s)"
    )
else:
    err_msg(
        f"Failed to build{do_tests_string} against Fenix (elapsed {time_diff_fenix:.2f}s)"
    )
if success_hnt:
    step_msg(
        f"Successfully built{did_tests_string} against HNT (elapsed {time_diff_hnt:.2f}s)"
    )
else:
    err_msg(
        f"Failed to build{do_tests_string} against HNT (elapsed {time_diff_hnt:.2f}s)"
    )