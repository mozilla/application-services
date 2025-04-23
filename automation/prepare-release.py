#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Prepare an Application-Services release
# Dependencies: yaml
# Usage: ./automation/prepare-release.py

import sys
import webbrowser
from datetime import datetime

from shared import (
    RefNames,
    check_output,
    ensure_working_tree_clean,
    fatal_err,
    get_moz_remote,
    run_cmd_checked,
    step_msg,
)

# Constants
VERSION_FILE = "version.txt"
CHANGELOG_FILE = "CHANGELOG.md"

# 1. Figure out which remote to push to
moz_remote = get_moz_remote()

# 2. Figure out the current version
with open(VERSION_FILE) as stream:
    cur_version = stream.read().strip()

major_version_number = int(cur_version.split(".")[0])
next_version_number = major_version_number + 1
release_version = f"{major_version_number}.0"
refs = RefNames(major_version_number, 0)

# 3. Create a new branch based on the branch we want to release from.

if check_output(["git", "rev-parse", "--abbrev-ref", "HEAD"]).strip() != refs.main:
    fatal_err(f"automation/prepare-release.py must be run from the {refs.main} branch")
ensure_working_tree_clean()

step_msg(f"Creating {refs.release}")
run_cmd_checked(["git", "checkout", "-b", refs.release])
run_cmd_checked(["git", "push", moz_remote, refs.release])

# 4. Create a PR to update the release branch

step_msg(f"checkout {refs.release_pr}")
run_cmd_checked(["git", "checkout", "-b", refs.release_pr])

step_msg(f"Bumping version in {VERSION_FILE}")
new_version = f"{major_version_number}.0"

with open(VERSION_FILE, "w") as stream:
    stream.write(new_version)

step_msg(f"updating {CHANGELOG_FILE}")
with open(CHANGELOG_FILE) as stream:
    changelog = stream.read().splitlines()

if changelog[0] != f"# v{major_version_number}.0 (In progress)":
    fatal_err(f"Unexpected first changelog line: {changelog[0]}")
today_date = datetime.today().strftime("%Y-%m-%d")

for i in range(10):
    if changelog[i] == "[Full Changelog](In progress)":
        changelog[i] = (
            f"[Full Changelog](https://github.com/mozilla/application-services/compare/"
            f"{refs.previous_version_tag}...{refs.version_tag})"
        )
        break
else:
    fatal_err("Can't find `[Full Changelog](In progress)` line in CHANGELOG.md")

changelog[0] = f"# v{major_version_number}.0 (_{today_date}_)"
with open(CHANGELOG_FILE, "w") as stream:
    stream.write("\n".join(changelog))
    stream.write("\n")

step_msg("Creating a commit with the changes")
run_cmd_checked(["git", "add", CHANGELOG_FILE])
run_cmd_checked(["git", "add", VERSION_FILE])
run_cmd_checked(["git", "commit", "-m", f"Cut release v{release_version}"])

step_msg(f"Pushing {refs.release_pr}")

# 5. Create a PR to update main

step_msg(f"checkout {refs.main}")
run_cmd_checked(["git", "checkout", refs.main])
run_cmd_checked(["git", "checkout", "-b", refs.start_release_pr])

step_msg(f"Bumping version in {VERSION_FILE}")
new_version = f"{next_version_number}.0a1"

with open(VERSION_FILE, "w") as stream:
    stream.write(new_version)

step_msg(f"UPDATING {CHANGELOG_FILE}")
changelog[0:0] = [
    f"# v{next_version_number}.0 (In progress)",
    "",
    "[Full Changelog](In progress)",
    "",
]
with open(CHANGELOG_FILE, "w") as stream:
    stream.write("\n".join(changelog))
    stream.write("\n")

step_msg("Creating a commit with the changes")
run_cmd_checked(["git", "add", CHANGELOG_FILE])
run_cmd_checked(["git", "commit", "-m", f"Start release v{next_version_number}"])

print()
response = input(
    "Great! Would you like to push and open the two pull-requests? ([Y]/N)"
).lower()
if response not in ("y", "", "yes"):
    sys.exit(0)

run_cmd_checked(["git", "push", moz_remote, refs.release_pr])
run_cmd_checked(["git", "push", moz_remote, refs.start_release_pr])

webbrowser.open_new_tab(
    f"https://github.com/mozilla/application-services/compare/{refs.release}...{refs.release_pr}"
)
webbrowser.open_new_tab(
    f"https://github.com/mozilla/application-services/compare/{refs.main}...{refs.start_release_pr}"
)
run_cmd_checked(["git", "checkout", refs.main])
