#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import argparse
import os
import subprocess


def main():
    args = parse_args()
    git_checkout(firefox_android_repo(args), args.branch)
    local_properties = "\n".join(
        [
            "autoPublish.application-services.dir=../../",
        ]
    )
    print("Local properties:")
    print(local_properties)

    write_local_properties(
        "firefox-android/android-components/local.properties", local_properties
    )
    write_local_properties("firefox-android/fenix/local.properties", local_properties)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Setup a firefox-android branch build in taskcluster"
    )
    parser.add_argument("owner", help="firefox-android repository owner")
    parser.add_argument("branch", help="firefox-android branch")
    return parser.parse_args()


def firefox_android_repo(args):
    return f"https://github.com/{args.owner}/firefox-android"


def git_checkout(url, branch):
    subprocess.check_call(
        [
            "git",
            "clone",
            "--branch",
            branch,
            "--recurse-submodules",
            "--depth",
            "1",
            "--",
            url,
        ]
    )


def write_local_properties(path, local_properties):
    path = os.path.abspath(path)
    print(f"Writing local properties to {path}")
    with open(path, "w") as f:
        f.write(local_properties)


if __name__ == "__main__":
    main()
