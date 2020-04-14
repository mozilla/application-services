#!/usr/bin/env python3

# Purpose: Publish android packages to local maven repo, but only if changed since last publish.
# Dependencies: None
# Usage: ./automation/publish_to_maven_local_if_modified.py

import os
import time
import hashlib
import argparse
from shared import run_cmd_checked, find_app_services_root, fatal_err
import re

LAST_CONTENTS_HASH_FILE = ".lastAutoPublishContentsHash"

parser = argparse.ArgumentParser(description="Publish android packages to local maven repo, but only if changed since last publish")
parser.parse_args()

root_dir = find_app_services_root()
if str(root_dir) != os.path.abspath(os.curdir):
    fatal_err(f"This only works if run from the repo root ({root_dir!r} != {os.path.abspath(os.curdir)!r})")

# Calculate a hash reflecting the current state of the repo.

contents_hash = hashlib.sha256()

contents_hash.update(
    run_cmd_checked(["git", "rev-parse", "HEAD"], capture_output=True).stdout
)
contents_hash.update(b"\x00")

# Git can efficiently tell us about changes to tracked files, including
# the diff of their contents, if you give it enough "-v"s.

changes = run_cmd_checked(["git", "status", "-v", "-v"], capture_output=True).stdout
contents_hash.update(changes)
contents_hash.update(b"\x00")

# But unfortunately it can only tell us the names of untracked
# files, so we need to slurp their contents in for ourselves.

changes_lines = iter(ln.strip() for ln in changes.split(b"\n"))
try:
    ln = next(changes_lines)
    while not ln.startswith(b"Untracked files:"):
        ln = next(changes_lines)
    ln = next(changes_lines) # skip instruction about using `git add`
    ln = next(changes_lines)
    while ln:
        with open(ln, "rb") as f:
            contents_hash.update(f.read())
        contents_hash.update(b"\x00")
        ln = next(changes_lines)
except StopIteration:
    pass
contents_hash.update(b"\x00")

contents_hash = contents_hash.hexdigest()

# If the contents hash has changed since last publish, re-publish.

last_contents_hash = ""
try:
    with open(LAST_CONTENTS_HASH_FILE) as f:
        last_contents_hash = f.read().strip()
except FileNotFoundError:
    pass

if contents_hash == last_contents_hash:
    print("Contents have not changed, no need to publish")
else:
    print("Contents have changed, publishing")
    run_cmd_checked(["./gradlew", "publishToMavenLocal", f"-Plocal={time.time_ns()}"])
    with open(LAST_CONTENTS_HASH_FILE, "w") as f:
        f.write(contents_hash)
        f.write("\n")
