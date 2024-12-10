#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""
Clean the world.

Note that this is probably only a partial clean - in particular, no attempt
is made to clean the iOS/Swift world, because markh doesn't know what that
involves.

Please make a PR with anything you notice that should be cleaned but isn't!
"""

import argparse
import shlex
import shutil
import subprocess
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent


def run_command(dry_run, cmdline, **kwargs):
    print("Executing:", " ".join(shlex.quote(str(part)) for part in cmdline))
    if not dry_run:
        subprocess.check_call(cmdline, **kwargs)


def find_generated_directories(look_dir):
    for child in look_dir.iterdir():
        if child.name == "support":
            for sub in find_generated_directories(child):
                yield sub
        else:
            # `android/build` directories should be removed.
            sub = child / "android" / "build"
            if sub.is_dir():
                yield sub
            # TODO: ios/swift?


def clean_android(dry_run):
    # pathlib.Path will join "." and "gradlew" as "gradlew", which doesn't
    # work as "." is not on the path!
    gradlew = (PROJECT_ROOT / "gradlew").resolve()
    # Running gradle is fairly likely to fail if the environment isn't setup,
    # so we ignore errors there...
    try:
        run_command(dry_run, [gradlew, "clean"], shell=True)
    except subprocess.CalledProcessError:
        print("`./gradle clean` failed, but looking for other Android stuff...")
    # ... and still try and find obviously generated directories.
    for to_rm in find_generated_directories(PROJECT_ROOT / "components"):
        print("Removing:", to_rm)
        if not dry_run:
            shutil.rmtree(to_rm)


def parse_args():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "-n",
        "--dry-run",
        dest="dry_run",
        action="store_true",
        help="show what would be executed/removed without actually doing it.",
    )
    return parser.parse_args()


def main():
    args = parse_args()
    run_command(args.dry_run, ["cargo", "clean"])
    clean_android(args.dry_run)
    # TODO: add swift etc.
    print("We should be clean! (except for iOS - fix me? :)")


if __name__ == "__main__":
    main()
