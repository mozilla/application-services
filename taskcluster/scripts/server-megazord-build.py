#!/usr/bin/python3

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
PATH_NOT_SPECIFIED = pathlib.Path("/not specified").resolve()
PWD = pathlib.Path().resolve()
DEBUG = False
TARGET_DETECTOR_SCRIPT = SRC_ROOT / "taskcluster" / "scripts" / "detect-target.sh"


def main():
    args = parse_args()
    megazord = args.megazord
    target = args.target
    out_dir = PWD / args.out_dir

    _dir = SRC_ROOT / "megazords" / megazord
    if not _dir.is_dir():
        raise NotADirectoryError(f"Megazord {megazord} does not exist to build")

    if DEBUG:
        temp_dir = None
        dist_dir = PWD / "dist"
        os.makedirs(dist_dir, exist_ok=True)
    else:
        temp_dir = tempfile.TemporaryDirectory()
        dist_dir = pathlib.Path(temp_dir.name)

    try:
        filename = _build_shared_library(megazord, target, dist_dir)
        if _target_matches_host(target):
            _run_python_tests(megazord, dist_dir)

        _prepare_artifact(megazord, target, filename, dist_dir)

        if str(out_dir) != str(PATH_NOT_SPECIFIED):
            os.makedirs(out_dir, exist_ok=True)
            _create_artifact(megazord, target, dist_dir, out_dir)
    finally:
        if not DEBUG:
            temp_dir.cleanup()


def _build_shared_library(megazord, target, dist_dir):
    env = os.environ.copy()
    binary = megazord.replace("-", "_")

    if "-linux" in target:
        filename = f"lib{binary}.so"
    elif "-darwin" in target:
        filename = f"lib{binary}.dylib"
    elif "-win" in target:
        filename = f"{binary}.dll"
    else:
        raise NotImplementedError("Only targets for linux, darwin or windows available")

    if "-musl" in target:
        env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C target-feature=-crt-static"
        if _host_os() == "unknown-linux":
            env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C link-arg=-lgcc"
        elif _host_os() == "apple-darwin":
            if "x86_64" in target:
                env["CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"] = (
                    "x86_64-linux-musl-gcc"
                )
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
    _patch_uniffi_tomls()

    library_path = SRC_ROOT / "target" / target / "release" / filename

    # Generate the Python FFI. We do this with `uniffi-bindgen-library-mode` so we don't have to specify the UDL or the uniffi.toml file.
    # Use the `-l` flag rather than `-m` since we want to specify a particular target.
    subprocess.check_call(
        [
            "cargo",
            "uniffi-bindgen-library-mode",
            "-l",
            library_path.as_posix(),
            "python",
            dist_dir,
        ],
        env=env,
        cwd=SRC_ROOT,
    )

    # Move the .so file to the dist_directory
    shutil.move(
        SRC_ROOT / "target" / target / "release" / filename, dist_dir / filename
    )

    return filename


def _patch_uniffi_tomls():
    _replace_text(
        SRC_ROOT / "components" / "support" / "nimbus-fml" / "uniffi.toml",
        "\ncdylib_name",
        "\n# cdylib_name",
    )
    _replace_text(
        SRC_ROOT / "components" / "nimbus" / "uniffi.toml",
        "\ncdylib_name",
        "\n# cdylib_name",
    )


def _replace_text(filename, search, replace):
    with open(filename) as file:
        data = file.read()
    data = data.replace(search, replace)
    with open(filename, "w") as file:
        file.write(data)


def _run_python_tests(megazord, dist_dir):
    env = os.environ.copy()
    existing = env.get("PYTHONPATH", None)
    dist_path = [str(dist_dir)] + _python_sources(megazord)
    if existing is None:
        env["PYTHONPATH"] = ":".join(dist_path)
    else:
        env["PYTHONPATH"] = ":".join(existing.split(":") + dist_path)

    test_dirs = _python_tests(megazord)
    for d in test_dirs:
        subprocess.check_call(
            [
                "pytest",
                "-s",
                d,
            ],
            env=env,
            cwd=SRC_ROOT,
        )


def _target_matches_host(target):
    return _host_os() in target and _host_machine() in target


def _host_machine():
    import platform

    m = platform.machine().lower()
    if m in ("i386", "amd64", "x86_64"):
        return "x86_64"
    elif m in ("arm64", "aarch64"):
        return "aarch64"
    else:
        return m


def _host_os():
    import platform

    s = platform.system().lower()
    if "windows" in s:
        return "windows"
    elif "linux" in s:
        return "unknown-linux"
    elif "darwin" in s:
        return "apple-darwin"
    else:
        return s


def _python_sources(megazord):
    return _dirs(f"{SRC_ROOT}/megazords/{megazord}", ["python/lib", "python/src"])


def _python_tests(megazord):
    return _dirs(
        f"{SRC_ROOT}/megazords/{megazord}", ["tests/python-tests", "python/test"]
    )


def _dirs(prefix, list):
    return [f"{prefix}/{f}" for f in list if os.path.isdir(f"{prefix}/{f}")]


def _prepare_artifact(megazord, target, filename, dist_dir):
    for f in _python_sources(megazord):
        shutil.copytree(f, dist_dir)

    # Move the binary into a target specific directory.
    # This is so shared libraries for the same OS, but different architectures
    # don't overwrite one another.
    target_dir = dist_dir / target
    os.makedirs(target_dir, exist_ok=True)
    shutil.move(dist_dir / filename, target_dir / filename)

    scripts_dir = dist_dir / "scripts"
    os.makedirs(scripts_dir, exist_ok=True)
    shutil.copy(TARGET_DETECTOR_SCRIPT, scripts_dir)


def _create_artifact(megazord, target, dist_dir, out_dir):
    archive = out_dir / f"{megazord}-{target}.zip"
    subprocess.check_call(
        [
            "zip",
            archive,
            "-r",
            ".",
            "-x",
            "*/__pycache__/*",
            "__pycache__/*",
        ],
        cwd=dist_dir,
    )

    print(f"Archive complete: {archive}")


def parse_args():
    parser = argparse.ArgumentParser(prog="server-megazord-build.py")
    parser.add_argument("megazord")
    parser.add_argument("target")
    parser.add_argument(
        "out_dir", nargs="?", type=pathlib.Path, default=PATH_NOT_SPECIFIED
    )

    return parser.parse_args()


if __name__ == "__main__":
    main()
