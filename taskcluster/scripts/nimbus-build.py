#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import argparse
import os
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from build_utils import needs_nss_setup, setup_nss_environment


def main():
    args = parse_args()
    binary = args.binary
    target = args.target
    os.makedirs(args.out_dir, exist_ok=True)
    filename = f"{binary}.exe" if "-windows-" in target else binary

    # Get the repository root
    src_root = pathlib.Path(
        subprocess.check_output(["git", "rev-parse", "--show-toplevel"])
        .decode("utf8")
        .strip()
    ).resolve()

    env = os.environ.copy()

    # Setup NSS environment if needed for this target
    if needs_nss_setup(target):
        setup_nss_environment(env, target, src_root)

    if target == "aarch64-unknown-linux-gnu":
        env["RUSTFLAGS"] = "-C linker=aarch64-linux-gnu-gcc"

    subprocess.check_call(
        [
            "cargo",
            "build",
            # Need to specify both --package and --bin, or else cargo will enable the features for
            # all binaries, which will probably lead to a failure when trying to build NSS.
            "--package",
            binary,
            "--bin",
            binary,
            "--release",
            "--target",
            target,
        ],
        env=env,
    )
    subprocess.check_call(
        [
            "zip",
            "-r",
            f"../build/{binary}-{target}.zip",
            pathlib.Path(target).joinpath("release", filename),
        ],
        cwd="target",
    )


def parse_args():
    parser = argparse.ArgumentParser(prog="nimbus-build.py")
    parser.add_argument("out_dir", type=pathlib.Path)
    parser.add_argument("binary")
    parser.add_argument("target")
    return parser.parse_args()


if __name__ == "__main__":
    main()
