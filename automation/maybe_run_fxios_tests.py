#!/usr/bin/env python3

# Purpose: Run firefox iOS Smoke tests if the last commit of a pull-request contains the [ci smoketest firefox-ios] string.
#          A different branch of firefox-ios can be targeted by using the [ci smoketest firefox-ios=<branch>] syntax.
#          Meant to be used by a CircleCI runner.
# Dependencies: None
# Usage: ./automation/maybe_run_fxios_tests.py

import argparse
from shared import run_cmd_checked, find_app_services_root
import re

parser = argparse.ArgumentParser(description="Run firefox iOS Smoke tests if the last commit in a PR contains the [ci smoketest firefox-ios] string.")
parser.add_argument("base_revision", help="The base revision this PR is based upon.")
args = parser.parse_args()
base_revision = args.base_revision

root_dir = find_app_services_root()
# CircleCI doesn't fetch the base revision, do it.
run_cmd_checked(["git", "fetch", "origin", base_revision], cwd=root_dir)

# Find out the commit message of the headmost revision of the PR.
commit_msg = run_cmd_checked(
    ["git", "rev-list", "--format=%B", "--max-count=1", f"{base_revision}..HEAD"],
    capture_output=True,
    text=True,
    cwd=root_dir
).stdout

match = re.search("\\[ci smoketest firefox-ios(?:=(.+))?\\]", commit_msg)
if match is None:
    print("Trigger tag not present, exiting.")
    exit(0)
branch = match[1]

# TODO: It's a bit dumb to call this on command line, we should make a helper function instead.
cmd = ["./automation/smoke-test-firefox-ios.py"]
if branch:
    cmd.extend(["--branch", branch])

run_cmd_checked(cmd, cwd=root_dir)
