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
#       --action            => Can be either `run-tests` (default) or `build-without-testing`
#       --use-local-repo    => Use a local firefox-ios repository instead (at the provided path). Exclusive with `remote-repo-url`.
#       --verbose           => Includes the stdout of subprocesses (like the xcodebuild output, or other bootstrapping scripts)
#       --branch            => What branch of the firefox-ios repo to use
# TODO: args

import argparse
import time
from shared import err_msg, step_msg
from build_against_fenix import build_against_fenix
from build_against_ios import build_against_ios
from build_against_hnt import build_against_hnt

parser = argparse.ArgumentParser(
    description="Run groups of tests against this application-services working tree."
)

group = parser.add_mutually_exclusive_group()
parser.add_argument(
    "--firefox-dir",
    required=True,
    help="Path to existing mozilla-central directory.",
)
parser.add_argument("--verbose", help="Includes subprocess running logs.", action=argparse.BooleanOptionalAction)
parser.add_argument('--allow-clears', 
                    help="Clear existing uniffi bindings, swift files, and so on during the various build processes.",
                    action=argparse.BooleanOptionalAction)
parser.add_argument(
    "--action",
    required=True,
    choices=["run-tests", "build-without-testing"],
    help="Run the following action once firefox-ios is set up.",
)

args = parser.parse_args()
firefox_dir = args.firefox_dir
verbose = args.verbose if args.verbose else False
allow_clears = args.allow_clears
action = args.action

# Build against iOS
# TODO: We *may* be able to run this one concurrently to the other two?
start_time_ios = time.time()
success_ios = build_against_ios(None, None, clear_previous_bindings=allow_clears, clean_ios_caches=allow_clears, verbose=verbose, action=action)
time_diff_ios = time.time() - start_time_ios

# Build against Fenix
start_time_fenix = time.time()
success_fenix = build_against_fenix(firefox_dir, None, None, None, clear_bindings=allow_clears, verbose=verbose, action=action)
time_diff_fenix = time.time() - start_time_fenix


did_tests_string = "" if action != "run-tests" else " (and tested)"
do_tests_string = "" if action != "run-tests" else " (and test)"
step_msg("Finished building. Results:")
if success_ios:
    step_msg(f"Successfully built{did_tests_string} against iOS (elapsed {time_diff_ios:.2f}s)")
else:
    err_msg(f"Failed to build{do_tests_string} against iOS (elapsed {time_diff_ios:.2f}s)")
if success_fenix:
    step_msg(f"Successfully built{did_tests_string} against Fenix (elapsed {time_diff_fenix:.2f}s)")
else:
    err_msg(f"Failed to build{do_tests_string} against Fenix (elapsed {time_diff_fenix:.2f}s)")

# TODO: This is currently skipped for draft form of PR, pending discussion.
# if skipped_hnt:
#     print("Skipped building against HNT. Pass `--use-monorepo-a-s` to build using the `mozilla-central` A-S tree")
# elif success_hnt:
#     step_msg(f"Successfully built{did_tests_string} against HNT in (elapsed {time_diff_hnt:.2f}s)")
# else:
#     err_msg(f"Failed to build{do_tests_string} against HNT (elapsed {time_diff_hnt:.2f}s)")
