#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Run Firefox-iOS tests against this application-services working tree.
# Usage: ./automation/smoke-test-fxios.py

import argparse
import subprocess
import sys
import tempfile

from shared import fatal_err, find_app_services_root, run_cmd_checked, step_msg

parser = argparse.ArgumentParser(
    description="Run Firefox-iOS tests against this application-services working tree."
)
group = parser.add_mutually_exclusive_group()
group.add_argument(
    "--use-local-repo",
    metavar="LOCAL_REPO_PATH",
    help="Use a local copy of firefox-ios instead of cloning it.",
)
group.add_argument(
    "--remote-repo-url",
    metavar="REMOTE_REPO_PATH",
    help="Clone a different firefox-ios repository.",
)
parser.add_argument("--branch", help="Branch of firefox-ios to use.")
parser.add_argument(
    "--action",
    choices=["open-project", "run-tests", "do-nothing"],
    help="Run the following action once firefox-ios is set up.",
)

DEFAULT_REMOTE_REPO_URL = "https://github.com/mozilla-mobile/firefox-ios.git"
DEFAULT_RCS_REPO_URL = "https://github.com/mozilla/rust-components-swift.git"

args = parser.parse_args()
firefox_ios_branch = args.branch
local_repo_path = args.use_local_repo
remote_repo_url = args.remote_repo_url
action = args.action

ios_repo_path = local_repo_path
appservices_path = find_app_services_root()

if local_repo_path is None:
    ios_repo_path = tempfile.mkdtemp(suffix="-fxios")
    if remote_repo_url is None:
        remote_repo_url = DEFAULT_REMOTE_REPO_URL
    step_msg(f"Cloning {remote_repo_url}")
    run_cmd_checked(["git", "clone", remote_repo_url, ios_repo_path])
    if firefox_ios_branch is not None:
        run_cmd_checked(["git", "checkout", firefox_ios_branch], cwd=ios_repo_path)
elif firefox_ios_branch is not None:
    fatal_err(
        "Cannot specify branch when using a local repo; check it out locally and try again."
    )

step_msg("Cloning rust-components-swift")
rcs_repo_path = tempfile.mkdtemp(suffix="-rcs")
run_cmd_checked(["git", "clone", DEFAULT_RCS_REPO_URL, rcs_repo_path])

step_msg("Setting up iOS to use the local application services")
run_cmd_checked(
    ["./rust_components_local.sh", "-a", appservices_path, rcs_repo_path],
    cwd=ios_repo_path,
)

step_msg("Running the firefox-ios bootstrap script")
run_cmd_checked(["./bootstrap.sh"], cwd=ios_repo_path)


if action == "do-nothing":
    sys.exit(0)
elif action == "open-project":
    run_cmd_checked(["open", "Client.xcodeproj"], cwd=ios_repo_path)
elif action == "run-tests" or action is None:
    # TODO: we specify scheme = Fennec, but it might be wrong? Check with iOS peeps.
    step_msg("Running firefox-ios tests")
    subprocess.run(
        """\
    set -o pipefail && \
    xcodebuild \
    -workspace ./Client.xcodeproj/project.xcworkspace \
    -scheme Fennec \
    -sdk iphonesimulator \
    -destination 'platform=iOS Simulator,name=iPhone 14' \
    test | \
    tee raw_xcodetest.log | \
    xcpretty && exit "${PIPESTATUS[0]}"
    """,
        cwd=ios_repo_path,
        shell=True,
        check=False,
    )
else:
    print("Sorry I did not understand what you wanted. Good luck!")
