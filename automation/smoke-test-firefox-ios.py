#!/usr/bin/env python3

# Purpose: Run Firefox-iOS tests against this application-services working tree.
# Usage: ./automation/smoke-test-firefox-ios.py

import argparse
import subprocess
import tempfile
from pathlib import Path
from shared import step_msg, fatal_err, run_cmd_checked, find_app_services_root

parser = argparse.ArgumentParser(description="Run Firefox-iOS tests against this application-services working tree.")
group = parser.add_mutually_exclusive_group()
group.add_argument("--use-local-repo",
                    metavar="LOCAL_REPO_PATH",
                    help="Use a local copy of firefox-ios instead of cloning it.")
group.add_argument("--remote-repo-url",
                    metavar="REMOTE_REPO_PATH",
                    help="Clone a different firefox-ios repository.")
parser.add_argument("--branch",
                    help="Branch of firefox-ios to use.")
parser.add_argument("--action",
                    choices=["open-project", "run-tests", "do-nothing"],
                    help="Run the following action once firefox-ios is set up.")

DEFAULT_REMOTE_REPO_URL="https://github.com/mozilla-mobile/firefox-ios.git"

args = parser.parse_args()
firefox_ios_branch = args.branch
local_repo_path = args.use_local_repo
remote_repo_url = args.remote_repo_url
action = args.action

repo_path = local_repo_path

if local_repo_path is None:
    repo_path = tempfile.mkdtemp(suffix="-fxios")
    if remote_repo_url is None:
        remote_repo_url = DEFAULT_REMOTE_REPO_URL
    step_msg(f"Cloning {remote_repo_url}")
    run_cmd_checked(["git", "clone", remote_repo_url, repo_path])
    if firefox_ios_branch is not None:
        run_cmd_checked(["git", "checkout", firefox_ios_branch], cwd=repo_path)
elif firefox_ios_branch is not None:
    fatal_err("Cannot specify branch when using a local repo; check it out locally and try again.")

if not Path(repo_path, "Carthage").exists():
    step_msg("Carthage folder not present. Running the firefox-ios bootstrap script")
    run_cmd_checked(["./bootstrap.sh"], cwd=repo_path)
step_msg("Running carthage substitution script")
run_cmd_checked(["./appservices_local_dev.sh", "enable", find_app_services_root()], cwd=repo_path)

if action == "do-nothing":
    exit(0)
elif action == "open-project":
    run_cmd_checked(["open", "Client.xcodeproj"], cwd=repo_path)
elif action == "run-tests" or action is None:
    # TODO: we specify scheme = Fennec, but it might be wrong? Check with iOS peeps.
    step_msg("Running firefox-ios tests")
    subprocess.run("""\
    set -o pipefail && \
    xcodebuild \
    -workspace ./Client.xcodeproj/project.xcworkspace \
    -scheme Fennec \
    -sdk iphonesimulator \
    -destination 'platform=iOS Simulator,name=iPhone 8' \
    test | \
    tee raw_xcodetest.log | \
    xcpretty && exit "${PIPESTATUS[0]}"
    """, cwd=repo_path, shell=True)
else:
    print("Sorry I did not understand what you wanted. Good luck!")
