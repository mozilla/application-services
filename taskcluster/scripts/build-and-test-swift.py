#!/usr/bin/python3

from collections import namedtuple
import argparse
import subprocess
import pathlib
import os

# Repository root dir
ROOT_DIR = pathlib.Path(__file__).parent.parent.parent
# List of udl_paths to generate bindings for
BINDINGS_UDL_PATHS = [
    "components/autofill/src/autofill.udl",
    "components/support/error/src/errorsupport.udl",
    "components/fxa-client/src/fxa_client.udl",
    "components/logins/src/logins.udl",
    "components/nimbus/src/nimbus.udl",
    "components/places/src/places.udl",
    "components/push/src/push.udl",
    "components/sync_manager/src/syncmanager.udl",
    "components/tabs/src/tabs.udl",
    "components/sync15/src/sync15.udl",
]

# List of udl_paths to generate bindings for
FOCUS_UDL_PATHS = [
    "components/nimbus/src/nimbus.udl",
    "components/support/error/src/errorsupport.udl",
]

# List of globs to copy the sources from
SOURCE_TO_COPY = [
    "components/nimbus/ios/Nimbus",
    "components/fxa-client/ios/FxAClient",
    "components/logins/ios/Logins",
    "components/tabs/ios/Tabs",
    "components/places/ios/Places",
    "components/sync15/ios/*",
    "components/sync_manager/ios/SyncManager",
    "components/rc_log/ios/*",
    "components/viaduct/ios/*",
]

# List of udl_paths to generate bindings for
FOCUS_SOURCE_TO_COPY = [
    "components/nimbus/ios/Nimbus",
    "components/rc_log/ios/*",
    "components/viaduct/ios/*",
]


def main():
    args = parse_args()
    ensure_dir(args.out_dir)
    run_tests(args)
    xcframework_build(args, "MozillaRustComponents.xcframework.zip")
    xcframework_build(args, "FocusRustComponents.xcframework.zip")
    generate_glean_metrics(args)
    generate_uniffi_bindings(args)
    copy_source_dirs(args)
    create_source_tarball(args)
    log("build complete")

def parse_args():
    parser = argparse.ArgumentParser(prog='build-and-test-swift.py')
    parser.add_argument('out_dir', type=pathlib.Path)
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
    subprocess.check_call(['cp', '-a', build_info.out_path, args.out_dir])

"""Generate Glean metrics.

Run this first, because it appears to delete any other .swift files in the output directory.
"""
def generate_glean_metrics(args):
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
    out_dir = args.out_dir / 'swift-components' / 'all' / 'Generated' / 'Metrics'
    focus_out_dir = args.out_dir / 'swift-components' / 'focus' / 'Generated' / 'Metrics'
    focus_glean_files = map(str, [ROOT_DIR / "components/nimbus/metrics.yaml"])
    firefox_glean_files = map(str, [ROOT_DIR / "components/nimbus/metrics.yaml", ROOT_DIR / "components/sync_manager/metrics.yaml", ROOT_DIR / "components/sync_manager/pings.yaml"])
    generate_glean_metrics_for_target(env, glean_script, out_dir, firefox_glean_files)
    generate_glean_metrics_for_target(env, glean_script, focus_out_dir, focus_glean_files)

def generate_glean_metrics_for_target(env, glean_script, out_dir, input_files):
    ensure_dir(out_dir)
    subprocess.check_call([
        str(glean_script),
        "-o", str(out_dir),
        *input_files
    ], env=env)

def generate_uniffi_bindings(args):
    out_dir = args.out_dir / 'swift-components' / 'all' / 'Generated'
    focus_out_dir = args.out_dir / 'swift-components' / 'focus' / 'Generated'

    ensure_dir(out_dir)

    generate_uniffi_bindings_for_target(out_dir, BINDINGS_UDL_PATHS)
    generate_uniffi_bindings_for_target(focus_out_dir, FOCUS_UDL_PATHS)

def generate_uniffi_bindings_for_target(out_dir, bindings_path):
    for udl_path in bindings_path:
        log(f"generating sources for {udl_path}")
        run_uniffi_bindgen(['generate', '-l', 'swift', '-o', out_dir, ROOT_DIR / udl_path])

def run_uniffi_bindgen(bindgen_args):
    all_args = [
        'cargo', 'run', '-p', 'embedded-uniffi-bindgen',
    ]
    all_args.extend(bindgen_args)
    subprocess.check_call(all_args, cwd=ROOT_DIR)

def copy_source_dirs(args):
    out_dir = args.out_dir / 'swift-components' / 'all'
    focus_out_dir = args.out_dir / 'swift-components' / 'focus'

    copy_sources(out_dir, SOURCE_TO_COPY)
    copy_sources(focus_out_dir, FOCUS_SOURCE_TO_COPY)

def copy_sources(out_dir, sources):
    ensure_dir(out_dir)
    for source in sources:
        log(f"copying {source}")
        for path in ROOT_DIR.glob(source):
            subprocess.check_call(['cp', '-r', path, out_dir])

def create_source_tarball(args):
    old_cwd = os.getcwd()
    os.chdir(args.out_dir)

    source_files = []
    for (dirpath, _, files) in os.walk('swift-components'):
        source_files.extend(
                pathlib.Path(dirpath) / f
                for f in files
                # Skip over Apple Double files
                if not f.startswith("._")
        )

    subprocess.check_call(['tar', 'acf', 'swift-components.tar.xz'] + source_files)
    os.chdir(old_cwd)

def ensure_dir(path):
    if not path.exists():
        os.makedirs(path)

def log(message):
    print()
    print(f'* {message}', flush=True)

if __name__ == '__main__':
    main()
