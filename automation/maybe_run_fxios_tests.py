#!/usr/bin/env python3

# Purpose: Run firefox iOS Smoke tests if the last commit in a PR contains the [ci smoketest firefox-ios] string.
# Dependencies: None
# Usage: ./automation/maybe_run_fxios_tests.py

from shared import run_cmd_checked, find_app_services_root
import re

root_dir = find_app_services_root()
# TODO: this will not work for forked repositories and PRs targeting another branch than master.
commit_msg = run_cmd_checked(["git", "rev-list", "--format=%B", "--max-count=1", "origin/master..HEAD"], capture_output=True, text=True, cwd=root_dir).stdout

match = re.search("\\[ci smoketest firefox-ios(?:=(.+))?\\]", commit_msg)
if match is None:
    print("No firefox-ios smoketest to run!")
    exit(0)

branch = match[1]

# TODO: It's a bit dumb to call this on cmd line, we should make a helper function instead.
cmd = ["./automation/smoke-test-firefox-ios.py"]
if branch:
    cmd.extend(["--branch", branch])

run_cmd_checked(cmd, cwd=root_dir)
