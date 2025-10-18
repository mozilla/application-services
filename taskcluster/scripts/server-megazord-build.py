#!/usr/bin/env python3

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import argparse
import os
import pathlib
import shutil
import subprocess
import tempfile

# Repository root dir
SRC_ROOT = pathlib.Path(
    subprocess.check_output(["git", "rev-parse", "--show-toplevel"])
    .decode("utf8")
    .strip()
).resolve()


def main():
    parser = argparse.ArgumentParser(description="Build a megazord library")
    parser.add_argument("megazord")
    parser.add_argument("target")
    parser.add_argument("dist_dir")
    args = parser.parse_args()

    megazord = args.megazord
    target = args.target
    dist_dir = args.dist_dir

    filename = _build_shared_library(megazord, target, dist_dir)
    checksum_filename = _create_checksum_file(filename)
    _create_zip(megazord, target, dist_dir, filename, checksum_filename)


def _build_shared_library(megazord, target, dist_dir):
    env = os.environ.copy()

    # Setup NSS environment for x86_64 desktop builds
    if target in ["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]:
        env["NSS_DIR"] = str(SRC_ROOT / "libs" / "desktop" / "linux-x86-64" / "nss")
        env["NSS_STATIC"] = "1"
        env["MOZ_AUTOMATION"] = "1"
        env["LIBCLANG_PATH"] = "/usr/lib/x86_64-linux-gnu"
        print(f"Using NSS from: {env['NSS_DIR']}")
    elif target == "x86_64-apple-darwin":
        env["NSS_DIR"] = str(SRC_ROOT / "libs" / "desktop" / "darwin" / "nss")
        env["NSS_STATIC"] = "1"
        env["MOZ_AUTOMATION"] = "1"
        env["LIBCLANG_PATH"] = "/Library/Developer/CommandLineTools/usr/lib"
        print(f"Using NSS from: {env['NSS_DIR']}")
    elif target == "aarch64-apple-darwin":
        env["NSS_DIR"] = str(SRC_ROOT / "libs" / "desktop" / "darwin" / "nss")
        env["NSS_STATIC"] = "1"
        env["MOZ_AUTOMATION"] = "1"
        env["LIBCLANG_PATH"] = "/Library/Developer/CommandLineTools/usr/lib"
        print(f"Using NSS from: {env['NSS_DIR']}")

    binary = megazord.replace("-", "_")

    if "-linux" in target:
        filename = f"lib{binary}.so"
    elif "-darwin" in target:
        filename = f"lib{binary}.dylib"
    else:
        filename = f"{binary}.dll"

    if "-musl" in target:
        if "x86_64" in target:
            env["TARGET_CC"] = "x86_64-linux-musl-gcc"
        elif "aarch64" in target:
            env["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER"] = (
                "aarch64-linux-musl-gcc"
            )

    if target == "x86_64-pc-windows-gnu":
        env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C panic=abort"
    elif target == "aarch64-unknown-linux-gnu":
        env["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"] = "aarch64-linux-gnu-gcc"

    subprocess.check_call(
        [
            "cargo",
            "build",
            "--manifest-path",
            f"{SRC_ROOT}/megazords/{megazord}/Cargo.toml",
            "--release",
            "--target",
            target,
        ],
        env=env,
        cwd=SRC_ROOT,
    )

    # This is only temporary, until cirrus uses pre-built binaries.
    if target == "x86_64-unknown-linux-gnu" and megazord == "cirrus":
        _copy_cirrus_cli_to(dist_dir)

    shutil.copy(f"target/{target}/release/{filename}", dist_dir)
    return filename


def _copy_cirrus_cli_to(output_dir):
    # This exists because cirrus currently has a Python client that invokes
    # the CLI. Once this is fully deprecated, we can remove this.
    source = SRC_ROOT / "components" / "support" / "cirrus" / "cirrus-cli.py"
    dest = pathlib.Path(output_dir) / "cirrus-cli.py"
    print(f"Copying {source} -> {dest}")
    shutil.copy(source, dest)


def _create_checksum_file(filename):
    checksum_filename = f"{filename}.sha256"
    with open(checksum_filename, "w") as f:
        f.write(
            subprocess.check_output(["shasum", "-a", "256", filename])
            .decode("utf8")
            .split()[0]
        )
    return checksum_filename


def _create_zip(megazord, target, dist_dir, filename, checksum_filename):
    """Create the final zip file

    The zip file includes the megazord library and also the native libraries
    that it depends on.  The goal is that consumers should be able to unpack
    the zip file and have everything they need.

    We currently only support this for linux.
    """
    zip_filename = f"{megazord}-{target}.zip"
    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = pathlib.Path(tmpdir)
        # Copy in megazord library and checksum
        shutil.copy(filename, tmpdir)
        shutil.copy(checksum_filename, tmpdir)
        if target == "x86_64-unknown-linux-gnu":
            # For Linux, also copy native dependencies.
            if megazord == "cirrus":
                # Cirrus also needs the CLI
                shutil.copy(f"{dist_dir}/cirrus-cli.py", tmpdir)
            lib_dir = tmpdir / "native-libs"
            lib_dir.mkdir()
            for lib_filename in _get_native_lib_filenames(filename):
                shutil.copy(lib_filename, lib_dir)
        # Create the zip file
        zip_path = pathlib.Path(dist_dir) / zip_filename
        subprocess.check_call(
            ["zip", "-r", zip_path, "."],
            cwd=tmpdir,
        )


def _get_native_lib_filenames(megazord_filename):
    """Get native library filenames that the megazord library depends on

    This function parses the `ldd` output to find the native dependencies.
    The ldd output looks like::

        linux-vdso.so.1 (0x00007ffd91fff000)
        libsqlite3.so.0 => /lib/x86_64-linux-gnu/libsqlite3.so.0 (0x00007fe734881000)
        libssl.so.3 => /lib/x86_64-linux-gnu/libssl.so.3 (0x00007fe7347d4000)
        ...

    We only care about the paths after `=>`, so we:
      - Filter out lines that don't have `=>`
      - Take the first item after `=>`
      - Ignore any paths under /lib or /lib64

    We don't want to include /lib or /lib64 since that's where the standard
    system libs live.  We do want to copy in any other paths, since those
    would be the libs that we fetch as part of the task.
    """
    output = subprocess.check_output(["ldd", megazord_filename]).decode("utf8")
    result = []
    for line in output.split("\n"):
        parts = line.split()
        try:
            index = parts.index("=>")
        except ValueError:
            # No `=>` in this line, skip it
            continue
        try:
            path = pathlib.Path(parts[index + 1])
        except IndexError:
            # No item after `=>`, skip this line
            continue
        if path.parts[1] not in ("lib", "lib64"):
            result.append(path)
    return result


if __name__ == "__main__":
    main()
