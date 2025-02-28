#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import io
import shutil
import subprocess
import sys
from configparser import RawConfigParser
from pathlib import Path


def main(args):
    if len(args) != 1:
        print("usage clean-gradle-autopublish.py [path-to-firefox-android]")
        sys.exit(1)
    ff_android_path = Path(args[0])
    if not path_looks_like_firefox_android(ff_android_path):
        print(f"{ff_android_path} does not look like a firefox-android repo")
        sys.exit(1)
    appservices_path = Path(__file__).parent.parent
    check_rust_targets(appservices_path)
    # Delete lastAutoPublishContentsHash to force gradle to rebuild/republish our maven packages
    delete_if_exists(appservices_path / ".lastAutoPublishContentsHash")
    # Delete the packages in our local maven repository as well
    delete_if_exists(
        Path.home().joinpath(".m2", "repository", "org", "mozilla", "appservices")
    )
    subprocess.run(["cargo", "clean"], cwd=appservices_path, check=False)
    subprocess.run(["./gradlew", "clean"], cwd=appservices_path, check=False)
    subprocess.run(
        ["./gradlew", "clean"], cwd=ff_android_path / "android-components", check=False
    )
    subprocess.run(["./gradlew", "clean"], cwd=ff_android_path / "fenix", check=False)


def path_looks_like_firefox_android(path):
    return (
        path.joinpath("android-components").exists() and path.joinpath("fenix").exists()
    )


def check_rust_targets(appservices_path):
    # config parser expects a header, but properties files don't have them.  So add one manually:
    f = io.StringIO()
    f.write("[main]\n")
    f.write((appservices_path / "local.properties").open().read())
    f.seek(0)
    config = RawConfigParser()
    config.read_file(f)
    rust_targets = config["main"].get("rust.targets")
    if rust_targets is not None:
        if "linux-x86-64" not in rust_targets.split(","):
            print(
                "rust.targets set in local.properties, but linux-x86-64 is not included."
            )
            print(
                "This will cause builds to fail, please fix this before running clean-gradle-autopublish.py"
            )
            sys.exit(1)
        print(f"rust targets set to: {rust_targets}")
        print("Note: this means that only APKs for those targets will work")
        input("\nPress enter to continue")


def delete_if_exists(path):
    if path.exists():
        if path.is_file():
            path.unlink()
        else:
            shutil.rmtree(path)


if __name__ == "__main__":
    main(sys.argv[1:])
