#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import argparse
import os
import pathlib
import subprocess
from collections import namedtuple

# Repository root dir
ROOT_DIR = pathlib.Path(__file__).parent.parent.parent

WRAPPER_DIR = pathlib.Path("megazords/ios-rust/Sources/MozillaRustComponentsWrapper/")
# List of globs to copy the sources from
SOURCE_TO_COPY = [
    WRAPPER_DIR / "Nimbus",
    WRAPPER_DIR / "FxAClient",
    WRAPPER_DIR / "Logins",
    WRAPPER_DIR / "Places",
    WRAPPER_DIR / "Sync15",
    WRAPPER_DIR / "SyncManager",
    WRAPPER_DIR / "Viaduct",
]

FOCUS_SOURCE_TO_COPY = [
    WRAPPER_DIR / "Nimbus",
    WRAPPER_DIR / "Viaduct",
]


def main():
    args = parse_args()
    ensure_dir(args.out_dir)
    ensure_dir(args.xcframework_dir)
    run_tests(args)
    xcframework_build(args, "MozillaRustComponents.xcframework.zip")
    xcframework_build(args, "FocusRustComponents.xcframework.zip")
    generate_glean_metrics(args)
    generate_uniffi_bindings(args)
    copy_source_dirs(args)
    log("build complete")


def parse_args():
    parser = argparse.ArgumentParser(prog="build-and-test-swift.py")
    parser.add_argument("out_dir", type=pathlib.Path)
    parser.add_argument("xcframework_dir", type=pathlib.Path)
    parser.add_argument("glean_work_dir", type=pathlib.Path)
    parser.add_argument("--force_build", action="store_true")
    return parser.parse_args()


def run_tests(args):
    # FIXME: this is currently failing with `Package.resolved file is corrupted or malformed; fix or
    # delete the file to continue`
    # subprocess.check_call([
    #     "automation/tests.py", "ios-tests"
    # ])
    pass


XCFrameworkBuildInfo = namedtuple(
    "XCFrameworkBuildInfo", "filename out_path build_command"
)
XCFRAMEWORK_BUILDS = [
    XCFrameworkBuildInfo(
        "MozillaRustComponents.xcframework.zip",
        "megazords/ios-rust/MozillaRustComponents.xcframework.zip",
        [
            "megazords/ios-rust/build-xcframework.sh",
            "--build-profile",
            "release",
        ],
    ),
    XCFrameworkBuildInfo(
        "FocusRustComponents.xcframework.zip",
        "megazords/ios-rust/focus/FocusRustComponents.xcframework.zip",
        [
            "megazords/ios-rust/build-xcframework.sh",
            "--build-profile",
            "release",
            "--focus",
        ],
    ),
]


def xcframework_build(args, filename):
    for build_info in XCFRAMEWORK_BUILDS:
        if build_info.filename == filename:
            break
    else:
        raise LookupError(f"No XCFrameworkBuildInfo for {filename}")

    # Build the XCFramework if it hasn't already been built (for example the `tests.py ios-tests`)
    if not os.path.exists(build_info.out_path) or args.force_build:
        subprocess.check_call(build_info.build_command)

    # Copy the XCFramework to our output directory
    subprocess.check_call(["cp", "-a", build_info.out_path, args.xcframework_dir])


"""Generate Glean metrics.

Run this first, because it appears to delete any other .swift files in the output directory.
"""


def generate_glean_metrics(args):
    # Make sure there's a python venv for glean to use
    venv_dir = args.glean_work_dir / ".venv"
    if not venv_dir.exists():
        log("setting up Glean venv")
        subprocess.check_call(["python3", "-m", "venv", str(venv_dir)])

    log("Running Glean for nimbus")
    # sdk_generator wants to be run from inside Xcode, so we set some env vars to fake it out.
    env = {
        "SOURCE_ROOT": str(args.glean_work_dir),
        "PROJECT": "MozillaAppServices",
        "GLEAN_PYTHON": "/usr/bin/env python3",
        "LC_ALL": "C.UTF-8",
        "LANG": "C.UTF-8",
        "PATH": os.environ["PATH"],
    }
    glean_script = (
        ROOT_DIR / "components/external/glean/glean-core/ios/sdk_generator.sh"
    )
    out_dir = args.out_dir / "all" / "Generated" / "Metrics"
    focus_out_dir = args.out_dir / "focus" / "Generated" / "Metrics"
    focus_glean_files = map(str, [ROOT_DIR / "components/nimbus/metrics.yaml"])
    firefox_glean_files = map(
        str,
        [
            ROOT_DIR / "components/nimbus/metrics.yaml",
            ROOT_DIR / "components/logins/metrics.yaml",
            ROOT_DIR / "components/sync_manager/metrics.yaml",
            ROOT_DIR / "components/sync_manager/pings.yaml",
        ],
    )
    generate_glean_metrics_for_target(env, glean_script, out_dir, firefox_glean_files)
    generate_glean_metrics_for_target(
        env, glean_script, focus_out_dir, focus_glean_files
    )


def generate_glean_metrics_for_target(env, glean_script, out_dir, input_files):
    ensure_dir(out_dir)
    subprocess.check_call(
        [str(glean_script), "-o", str(out_dir), *input_files], env=env
    )


def generate_uniffi_bindings(args):
    out_dir = args.out_dir / "all" / "Generated"
    focus_out_dir = args.out_dir / "focus" / "Generated"

    ensure_dir(out_dir)

    # Generate sources for Firefox
    generate_uniffi_bindings_for_target(out_dir, "megazord_ios")

    # Generate sources for Focus
    generate_uniffi_bindings_for_target(focus_out_dir, "megazord_focus")


def generate_uniffi_bindings_for_target(out_dir, megazord):
    log(f"generating sources for {megazord}")
    # We can't use the `-m` flag here because the megazord library was cross-compiled and the
    # `uniffi-bindgen-library-mode` tool can't handle that yet.  Instead, send one of the library
    # paths using the `-l` flag. Pick an arbitrary target, since the it doesn't affect the UniFFI
    # bindings.
    lib_path = f"target/aarch64-apple-ios/release/lib{megazord}.a"
    subprocess.check_call(
        ["cargo", "uniffi-bindgen-library-mode", "-l", lib_path, "swift", out_dir]
    )


def copy_source_dirs(args):
    out_dir = args.out_dir / "all"
    focus_out_dir = args.out_dir / "focus"

    copy_sources(out_dir, SOURCE_TO_COPY)
    copy_sources(focus_out_dir, FOCUS_SOURCE_TO_COPY)


def copy_sources(out_dir, sources):
    ensure_dir(out_dir)
    for source in sources:
        log(f"copying {source}")
        for path in ROOT_DIR.glob(str(source)):
            subprocess.check_call(["cp", "-r", path, out_dir])


def ensure_dir(path):
    if not path.exists():
        os.makedirs(path)


def log(message):
    print()
    print(f"* {message}", flush=True)


if __name__ == "__main__":
    main()
