#!/usr/bin/env python3

import argparse
import os
import pathlib
import subprocess

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out_dir")
    parser.add_argument("binary")
    parser.add_argument("target")
    args = parser.parse_args()
    target = args.target
    binary = args.binary
    os.makedirs(args.out_dir, exist_ok=True)
    filename = f"{binary}.exe" if "-windows-" in target else binary

    # Get the repository root
    src_root = pathlib.Path(
        subprocess.check_output(["git", "rev-parse", "--show-toplevel"])
        .decode("utf8")
        .strip()
    ).resolve()

    env = os.environ.copy()

    # Setup NSS environment for x86_64 desktop builds
    if target in ["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]:
        env["NSS_DIR"] = str(src_root / "libs" / "desktop" / "linux-x86-64" / "nss")
        env["NSS_STATIC"] = "1"
        env["MOZ_AUTOMATION"] = "1"
        env["LIBCLANG_PATH"] = "/usr/lib/x86_64-linux-gnu"
        print(f"Using NSS from: {env['NSS_DIR']}")
    elif target == "x86_64-apple-darwin":
        env["NSS_DIR"] = str(src_root / "libs" / "desktop" / "darwin" / "nss")
        env["NSS_STATIC"] = "1"
        env["MOZ_AUTOMATION"] = "1"
        env["LIBCLANG_PATH"] = "/Library/Developer/CommandLineTools/usr/lib"
        print(f"Using NSS from: {env['NSS_DIR']}")
    elif target == "aarch64-apple-darwin":
        env["NSS_DIR"] = str(src_root / "libs" / "desktop" / "darwin" / "nss")
        env["NSS_STATIC"] = "1"
        env["MOZ_AUTOMATION"] = "1"
        env["LIBCLANG_PATH"] = "/Library/Developer/CommandLineTools/usr/lib"
        print(f"Using NSS from: {env['NSS_DIR']}")

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
            "python3",
            "tools/nimbus-build-post.py",
            "--target",
            target,
            "--filename",
            filename,
            "--out-dir",
            args.out_dir,
        ]
    )


if __name__ == "__main__":
    main()
