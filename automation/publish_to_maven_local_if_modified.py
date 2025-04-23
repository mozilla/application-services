#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Publish android packages to local maven repo, but only if changed since last publish.
# Dependencies: None
# Usage: ./automation/publish_to_maven_local_if_modified.py

import argparse
import hashlib
import os
import shutil
import sys
import time

from shared import fatal_err, find_app_services_root, run_cmd_checked

LAST_CONTENTS_HASH_FILE = ".lastAutoPublishContentsHash"

GITIGNORED_FILES_THAT_AFFECT_THE_BUILD = ["local.properties"]

parser = argparse.ArgumentParser(
    description="Publish android packages to local maven repo, but only if changed since last publish"
)
parser.parse_args()

root_dir = find_app_services_root()
if str(root_dir) != os.path.abspath(os.curdir):
    fatal_err(
        f"This only works if run from the repo root ({root_dir!r} != {os.path.abspath(os.curdir)!r})"
    )

# This doesn't work on "native" windows, so let's get that out of the way now.
if sys.platform.startswith("win"):
    print("NOTE: The autoPublish workflows do not work on native windows.")
    print(
        "You must follow the instructions in /docs/howtos/setup-android-build-environment.md#using-windows"
    )
    print(
        "then, manually ensure that the following command has completed successfully in WSL:"
    )
    print(sys.argv)
    print(f"(from the '{root_dir}' directory)")
    print("Then restart the build")
    # We don't want to fail here - the intention is that building, eg,
    # android-components on native Windows still works, just that it prints the
    # warning above.
    sys.exit(0)

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
# files, and it won't tell us anything about files that are in
# .gitignore but can still affect the build.

untracked_files = []

changes_lines = iter(ln.strip() for ln in changes.split(b"\n"))
try:
    ln = next(changes_lines)
    # Skip the tracked files.
    while not ln.startswith(b"Untracked files:"):
        ln = next(changes_lines)
    # Skip instructional line about using `git add`.
    ln = next(changes_lines)
    # Now we're at the list of untracked files.
    ln = next(changes_lines)
    while ln:
        untracked_files.append(ln)
        ln = next(changes_lines)
except StopIteration:
    pass

untracked_files.extend(GITIGNORED_FILES_THAT_AFFECT_THE_BUILD)

# So, we'll need to slurp the contents of such files for ourselves.

for nm in untracked_files:
    try:
        with open(nm, "rb") as f:
            contents_hash.update(f.read())
    except (FileNotFoundError, IsADirectoryError):
        pass
    contents_hash.update(b"\x00")
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
    # Ensure rust changes get picked up. No idea why, but stale .so files under `intermediates` end up published.
    # Repro: 1) publish, 2) change rust, re-publish, 3) rust from first step is still in the artifacts; not all "dupe" .so files are identical.
    shutil.rmtree("./megazords/full/android/build/intermediates", ignore_errors=True)
    run_cmd_checked(["./gradlew", "publishToMavenLocal", f"-Plocal={time.time_ns()}"])
    with open(LAST_CONTENTS_HASH_FILE, "w") as f:
        f.write(contents_hash)
        f.write("\n")
