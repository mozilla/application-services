#!/usr/bin/python3

from collections import namedtuple
import argparse
import shutil
import subprocess
import pathlib
import os

# Repository root dir
ROOT_DIR = pathlib.Path(__file__).parent.parent.parent
# List of udl_paths to generate bindings for
BINDINGS_UDL_PATHS = [
    "components/autofill/src/autofill.udl",
    "components/crashtest/src/crashtest.udl",
    "components/fxa-client/src/fxa_client.udl",
    "components/logins/src/logins.udl",
    "components/nimbus/src/nimbus.udl",
    "components/places/src/places.udl",
    "components/push/src/push.udl",
    "components/support/error/src/errorsupport.udl",
    "components/sync_manager/src/syncmanager.udl",
]
# List of globs to copy the sources from
SOURCE_TO_COPY = [
    "components/nimbus/ios/Nimbus",
    "components/fxa-client/ios/FxAClient",
    "components/logins/ios/Logins",
    "components/tabs/ios/Tabs",
    "components/places/ios/Places",
    "components/sync15/ios/*",
    "components/rc_log/ios/*",
    "components/viaduct/ios/*",
]

def main():
    args = parse_args()
    ensure_dir(args.out_dir)
    run_tests(args)
    xcframework_build(args, "MozillaRustComponents.xcframework.zip")
    xcframework_build(args, "FocusRustComponents.xcframework.zip")
    generate_nimbus_metrics(args)
    generate_uniffi_bindings(args)
    copy_source_dirs(args)
    log("build complete")

def parse_args():
    parser = argparse.ArgumentParser(prog='build-and-test-swift.py')
    parser.add_argument('out_dir', type=pathlib.Path)
    parser.add_argument('xcframework_dir', type=pathlib.Path)
    parser.add_argument('glean_work_dir', type=pathlib.Path)
    return parser.parse_args()

def run_tests(args):
    # FIXME: this is currently failing with `Package.resolved file is corrupted or malformed; fix or
    # delete the file to continue`
    # subprocess.check_call([
    #     "automation/tests.py", "ios-tests"
    # ])
    pass

XCFrameworkBuildInfo = namedtuple("XCFrameworkBuildInfo", "filename out_path build_command")
XCFRAMEWORK_BUILDS = [
    XCFrameworkBuildInfo(
        'MozillaRustComponents.xcframework.zip',
        'megazords/ios-rust/MozillaRustComponents.xcframework.zip',
        [
            'megazords/ios-rust/build-xcframework.sh',
            '--build-profile',
            'release',
        ],
    ),
    XCFrameworkBuildInfo(
        'FocusRustComponents.xcframework.zip',
        'megazords/ios-rust/focus/FocusRustComponents.xcframework.zip',
        [
            'megazords/ios-rust/build-xcframework.sh',
            '--build-profile',
            'release',
            '--focus',
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
    if not os.path.exists(build_info.out_path):
        subprocess.check_call(build_info.build_command)

    # Copy the XCFramework to our output directory
    subprocess.check_call(['cp', '-a', build_info.out_path, args.xcframework_dir])

"""Generate Glean metrics.

Run this first, because it appears to delete any other .swift files in the output directory.
"""
def generate_nimbus_metrics(args):
    # Make sure there's a python venv for glean to use
    venv_dir = args.glean_work_dir / '.venv'
    if not venv_dir.exists():
        log("setting up Glean venv")
        subprocess.check_call(['python3', '-m', 'venv', str(venv_dir)])

    log("Running Glean for nimbus")
    # sdk_generator wants to be run from inside Xcode, so we set some env vars to fake it out.
    env = {
        'SOURCE_ROOT': str(args.glean_work_dir),
        'PROJECT': "MozillaAppServices",
        'GLEAN_PYTHON': '/usr/bin/python3',
        'LC_ALL': 'C.UTF-8',
        'LANG': 'C.UTF-8',
    }
    glean_script = ROOT_DIR / "components/external/glean/glean-core/ios/sdk_generator.sh"
    out_dir = args.out_dir / "glean-metrics"
    metrics_yaml = ROOT_DIR / "components/nimbus/metrics.yaml"
    ensure_dir(out_dir)
    subprocess.check_call([
        str(glean_script),
        "-o", str(out_dir),
        str(metrics_yaml)
    ], env=env)

def generate_uniffi_bindings(args):
    out_dir = args.out_dir / 'generated-swift-sources'
    ensure_dir(out_dir)

    for udl_path in BINDINGS_UDL_PATHS:
        log(f"generating sources for {udl_path}")
        run_uniffi_bindgen(['generate', '-l', 'swift', '--no-format', '-o', out_dir, ROOT_DIR / udl_path])

def run_uniffi_bindgen(bindgen_args):
    all_args = [
        'cargo', 'run', '-p', 'embedded-uniffi-bindgen',
    ]
    all_args.extend(bindgen_args)
    subprocess.check_call(all_args, cwd=ROOT_DIR)

def copy_source_dirs(args):
    out_dir = args.out_dir / 'swift-sources'
    ensure_dir(out_dir)

    for source in SOURCE_TO_COPY:
        log(f"copying {source}")
        for path in ROOT_DIR.glob(source):
            subprocess.check_call(['cp', '-r', path, out_dir])

def ensure_dir(path):
    if not path.exists():
        os.makedirs(path)

def log(message):
    print()
    print(f'* {message}', flush=True)

if __name__ == '__main__':
    main()
