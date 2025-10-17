#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""
This script builds server megazords.

Server megazords are shared libraries that contain multiple components.
They only target desktop platforms (Linux, macOS, Windows).
"""

import argparse
import os
import pathlib
import shutil
import subprocess

from build_utils import (
    get_src_root,
    setup_nss_environment,
    setup_libclang_path,
    needs_nss_setup,
)

SRC_ROOT = get_src_root()


def main():
    parser = argparse.ArgumentParser(description="Build server megazord")
    parser.add_argument("megazord", help="Name of the megazord to build")
    parser.add_argument("target", help="Rust target triple")
    parser.add_argument("dist_dir", help="Output directory for artifacts")
    args = parser.parse_args()

    _build_shared_library(args.megazord, args.target, args.dist_dir)


def _host_os():
    """Infer host OS from environment"""
    if os.path.exists("/System/Library/CoreServices/SystemVersion.plist"):
        return "apple-darwin"
    return "unknown-linux"


def _build_shared_library(megazord, target, dist_dir):
    """Build a shared library megazord for the specified target"""
    env = os.environ.copy()
    binary = megazord.replace("-", "_")

    # Set up NSS for desktop targets (not Windows)
    # Note: Server megazords don't target Android
    if needs_nss_setup(target) and "MOZ_FETCHES_DIR" in env:
        print(f"Setting up NSS for server megazord target: {target}")
        setup_nss_environment(env, target, SRC_ROOT)
        setup_libclang_path(env)
    elif not needs_nss_setup(target):
        print(f"Target {target} uses rust-hpke backend, skipping NSS setup")

    # Determine output filename based on platform
    if "-linux" in target:
        filename = f"lib{binary}.so"
    elif "-darwin" in target:
        filename = f"lib{binary}.dylib"
    elif "-windows" in target:
        filename = f"{binary}.dll"
    else:
        raise Exception(f"Unsupported platform: {target}")

    # Set up cross-compilation flags
    setup_cross_compile_flags_for_megazord(env, target)

    # Build the megazord
    print(f"Building {megazord} for {target}...")
    subprocess.check_call(
        [
            "cargo",
            "build",
            "--package",
            megazord,
            "--lib",
            "--release",
            "--target",
            target,
        ],
        env=env,
        cwd=SRC_ROOT,
    )

    # Copy the built library to the dist directory
    built_lib = SRC_ROOT / "target" / target / "release" / filename
    dist_path = pathlib.Path(dist_dir)
    dist_path.mkdir(parents=True, exist_ok=True)

    output_file = dist_path / filename
    print(f"Copying {built_lib} to {output_file}")
    shutil.copy(str(built_lib), str(output_file))

    print(f"Successfully built server megazord: {filename}")


def setup_cross_compile_flags_for_megazord(env, target):
    """Set up cross-compilation flags for server megazord builds"""
    if target == "x86_64-pc-windows-gnu":
        # Windows builds: use rust-hpke (no NSS, no bindgen)
        env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C panic=abort"

    elif target == "aarch64-unknown-linux-gnu":
        # ARM64 Linux cross-compilation
        env["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"] = "aarch64-linux-gnu-gcc"

        # Help bindgen/clang find headers when cross-compiling
        env["BINDGEN_EXTRA_CLANG_ARGS"] = (
            "--target=aarch64-unknown-linux-gnu "
            "--sysroot=/usr/aarch64-linux-gnu "
            "-I/usr/aarch64-linux-gnu/include"
        )


if __name__ == "__main__":
    main()
