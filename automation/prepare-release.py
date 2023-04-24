#!/usr/bin/env python3

# Purpose: Prepare an Application-Services release
# Dependencies: yaml
# Usage: ./automation/prepare-release.py

from datetime import datetime
import webbrowser
import yaml

from shared import (RefNames, get_moz_remote, step_msg, fatal_err, run_cmd_checked,
                    ensure_working_tree_clean, check_output)

# Constants
BUILDCONFIG_FILE = ".buildconfig-android.yml"
BUILDCONFIG_VERSION_FIELD = "libraryVersion"
CHANGELOG_FILE = "CHANGELOG.md"

# 1. Figure out which remote to push to
moz_remote = get_moz_remote()

# 2. Figure out the current version
with open(BUILDCONFIG_FILE, "r") as stream:
    build_config = yaml.safe_load(stream)

cur_version = build_config[BUILDCONFIG_VERSION_FIELD]
major_version_number = int(cur_version.split('.')[0])
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

step_msg(f"Bumping version in {BUILDCONFIG_FILE}")
build_config[BUILDCONFIG_VERSION_FIELD] = f"{major_version_number}.0"

with open(BUILDCONFIG_FILE, "w") as stream:
    yaml.dump(build_config, stream, sort_keys=False)

step_msg(f"updating {CHANGELOG_FILE}")
with open(CHANGELOG_FILE, "r") as stream:
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
    fatal_err(f"Can't find `[Full Changelog](In progress)` line in CHANGELOG.md")

changelog[0] = f"# v{major_version_number}.0 (_{today_date}_)"
with open(CHANGELOG_FILE, "w") as stream:
    stream.write("\n".join(changelog))

step_msg(f"Creating a commit with the changes")
run_cmd_checked(["git", "add", CHANGELOG_FILE])
run_cmd_checked(["git", "add", BUILDCONFIG_FILE])
run_cmd_checked(["git", "commit", "-m", f"Cut release v{release_version}"])

step_msg(f"Pushing {refs.release_pr}")

# 5. Create a PR to update main

step_msg(f"checkout {refs.main}")
run_cmd_checked(["git", "checkout", refs.main])
run_cmd_checked(["git", "checkout", "-b", refs.start_release_pr])

step_msg(f"UPDATING {CHANGELOG_FILE}")
changelog[0:0] = [
    f"# v{major_version_number+1}.0 (In progress)",
    "",
    "[Full Changelog](In progress)",
    ""
]
with open(CHANGELOG_FILE, "w") as stream:
    stream.write("\n".join(changelog))

step_msg(f"Creating a commit with the changes")
run_cmd_checked(["git", "add", CHANGELOG_FILE])
run_cmd_checked(["git", "commit", "-m", f"Start release v{major_version_number+1}"])

print()
response = input("Great! Would you like to push and open the two pull-requests? ([Y]/N)").lower()
if response != "y" and response != "" and response != "yes":
    exit(0)

run_cmd_checked(["git", "push", moz_remote, refs.release_pr])
run_cmd_checked(["git", "push", moz_remote, refs.start_release_pr])

webbrowser.open_new_tab(f"https://github.com/mozilla/application-services/compare/{refs.release}...{refs.release_pr}")
webbrowser.open_new_tab(f"https://github.com/mozilla/application-services/compare/{refs.main}...{refs.start_release_pr}")
run_cmd_checked(["git", "checkout", refs.main])
