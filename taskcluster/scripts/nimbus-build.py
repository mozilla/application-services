#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import argparse
import os
import pathlib
import subprocess

from build_utils import (
    get_src_root,
    setup_nss_environment,
    setup_libclang_path,
    needs_nss_setup,
    setup_cross_compile_aarch64_linux,
    setup_cross_compile_windows,
)

SRC_ROOT = get_src_root()


def main():
    parser = argparse.ArgumentParser(description="Build Nimbus binaries")
    parser.add_argument("out_dir", help="Output directory for build artifacts")
    parser.add_argument("binary", help="Binary name to build")
    parser.add_argument("target", help="Rust target triple")
    args = parser.parse_args()

    binary = args.binary
    target = args.target
    os.makedirs(args.out_dir, exist_ok=True)

    # Determine output filename
    filename = f"{binary}.exe" if "-windows-" in target else binary

    env = os.environ.copy()

    # Set up NSS for desktop targets (not Windows, not Android)
    if needs_nss_setup(target) and "MOZ_FETCHES_DIR" in env:
        print(f"Setting up NSS for target: {target}")
        setup_nss_environment(env, target, SRC_ROOT)
        setup_libclang_path(env)
    elif not needs_nss_setup(target):
        print(f"Target {target} uses rust-hpke backend, skipping NSS setup")

    # Set up cross-compilation flags
    setup_cross_compile_flags(env, target)

    # Build the binary
    print(f"Building {binary} for {target}...")
    subprocess.check_call(
        [
            "cargo",
            "build",
            "--package",
            binary,
            "--bin",
            binary,
            "--release",
            "--target",
            target,
        ],
        env=env,
        cwd=SRC_ROOT,
    )

    # Package the binary
    print(f"Packaging {binary}...")
    subprocess.check_call(
        [
            "zip",
            "-j",
            f"../build/{binary}-{target}.zip",
            pathlib.Path(target).joinpath("release", filename),
        ],
        cwd=SRC_ROOT / "target",
    )

    print(f"Successfully built and packaged {binary} for {target}")


def setup_cross_compile_flags(env, target):
    """Set up cross-compilation flags based on target"""
    if target == "aarch64-unknown-linux-gnu":
        setup_cross_compile_aarch64_linux(env)
    elif target == "x86_64-pc-windows-gnu":
        setup_cross_compile_windows(env)


if __name__ == "__main__":
    main()
