#!/usr/bin/env python3

# Purpose: Tag an Application-Services release
# Dependencies: yaml
# Usage: ./automation/tag-release.py [major-version-number]

import argparse
from datetime import datetime
import subprocess
import re
import webbrowser
import yaml

from shared import RefNames, get_moz_remote, step_msg, fatal_err, run_cmd_checked, ensure_working_tree_clean, check_output

parser = argparse.ArgumentParser(description="Tags an application-services release")
parser.add_argument("major-version-number", type=int)
args = parser.parse_args()
major_version_number = args.major_version_number
branch = f"{moz_remote}/release-v{major_version_number}"

step_msg(f"Getting version number")
moz_remote = get_moz_remote()
run_cmd_checked(["git", "fetch", moz_remote])
build_config = yaml.loads(check_output([
    "git",
    "show",
    f"{branch}:.buildconfig-android.yml",
]))
version = build_config[BUILDCONFIG_VERSION_FIELD]
tag = f"v{version}"

step_msg(f"Getting commit")
commit = check_output(["git", "rev-parse", branch])
logline = check_output(["git", "log", "-n1", "--oneline", branch])

print("Commit: {logline}")
print("Tag: {tag}")
response = input("Would you like to add the tag to the commit listed above? ([Y]/N)").lower()
if response != "y" and response != "" and response != "yes":
    exit(0)

run_cmd_checked(["git", "tag", branch, tag])
run_cmd_checked(["git", "push", moz_remote, tag])
