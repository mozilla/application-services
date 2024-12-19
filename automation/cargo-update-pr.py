#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Run cargo update and make a pull-request against main.
# Dependencies: None
# Usage: ./automation/cargo-update-pr.py

import argparse
import subprocess
import sys
import webbrowser
from datetime import datetime

from shared import ensure_working_tree_clean, fatal_err, run_cmd_checked, step_msg

parser = argparse.ArgumentParser(
    description="Run cargo update and make a pull-request against main"
)
parser.add_argument(
    "--remote",
    default="origin",
    help="The remote name that corresponds to the Application Services main repository.",
)

args = parser.parse_args()
remote = args.remote

ensure_working_tree_clean()

today_date = datetime.today().strftime("%Y-%m-%d")
branch_name = f"cargo-update-{today_date}"

step_msg(f"Check if branch {branch_name} already exists")

res = subprocess.run(
    ["git", "show-ref", "--verify", f"refs/heads/{branch_name}"],
    capture_output=True,
    check=False,
)

if res.returncode == 0:
    fatal_err(f"The branch {branch_name} already exists!")

step_msg(f"Updating remote {remote}")
run_cmd_checked(["git", "remote", "update", remote])

step_msg(f"Creating branch {branch_name}")
run_cmd_checked(["git", "checkout", "-b", branch_name, "--no-track", f"{remote}/main"])

step_msg("Running cargo update")
run_cmd_checked(["cargo", "update"])

while True:
    step_msg("Regenerating dependency summaries")
    res = subprocess.run(["./tools/regenerate_dependency_summaries.sh"], check=False)
    if res.returncode == 0:
        break
    print("It looks like the dependency summary generation script couldn't complete.")
    input("Please fix the issue then press any key to try again.")

step_msg("Creating a commit with the changes")
run_cmd_checked(
    ["git", "add", "-A"]
)  # We can use -A since we checked the working dir is clean.
run_cmd_checked(["git", "commit", "-m", "Run cargo update [ci full]"])

step_msg("Print summary of changes")
run_cmd_checked(["git", "show", "--stat"])

response = input(
    "Great! Would you like to push and open a pull-request? ([Y]/N)"
).lower()
if response not in ("y", "", "yes"):
    sys.exit(0)
run_cmd_checked(["git", "push", remote, branch_name])
webbrowser.open_new_tab(
    f"https://github.com/mozilla/application-services/pull/new/{branch_name}"
)
