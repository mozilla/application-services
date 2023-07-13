#!/usr/bin/env python3

# Purpose: Tag an Application-Services release
# Dependencies: yaml
# Usage: ./automation/tag-release.py [major-version-number]

import argparse

from shared import RefNames, get_moz_remote, step_msg, run_cmd_checked, check_output

parser = argparse.ArgumentParser(description="Tags an application-services release")
parser.add_argument("major_version_number", type=int)
args = parser.parse_args()
moz_remote = get_moz_remote()
major_version_number = args.major_version_number
branch = RefNames(major_version_number, 0).release

step_msg("Getting version number")
run_cmd_checked(["git", "fetch", moz_remote])
version = check_output([
    "git",
    "show",
    f"{moz_remote}/{branch}:version.txt",
]).strip()
tag = f"v{version}"

step_msg("Getting commit")
commit = check_output(["git", "rev-parse", f"{moz_remote}/{branch}"]).strip()
logline = check_output(["git", "log", "-n1", "--oneline", branch]).strip()

print(f"Branch: {branch}")
print(f"Commit: {logline}")
print(f"Tag: {tag}")
response = input("Would you like to add the tag to the commit listed above? ([Y]/N)").lower()
if response != "y" and response != "" and response != "yes":
    exit(0)

run_cmd_checked(["git", "tag", tag, commit])
run_cmd_checked(["git", "push", moz_remote, tag])
