# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# These transforms perform additional setup that could not have been done in the Dockerfile
# due to this Taskcluster bug: https://bugzilla.mozilla.org/show_bug.cgi?id=1587611

from __future__ import absolute_import, print_function, unicode_literals
from taskgraph.transforms.base import TransformSequence

for_run_task = TransformSequence()

SCRIPT = '''
export RUST_BACKTRACE='1'
export RUSTFLAGS='-Dwarnings'
export CARGO_INCREMENTAL='0'
export CI='1'
export CCACHE='sccache'
export RUSTC_WRAPPER='sccache'
export SCCACHE_IDLE_TIMEOUT='1200'
export SCCACHE_CACHE_SIZE='40G'
export SCCACHE_ERROR_LOG='/build/sccache.log'
export RUST_LOG='sccache=info'

rustup toolchain install stable
rustup default stable
rustup target add x86_64-linux-android i686-linux-android armv7-linux-androideabi aarch64-linux-android
test -d $ANDROID_NDK_TOOLCHAIN_DIR/arm-$ANDROID_NDK_API_VERSION    || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="arm"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/arm-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
test -d $ANDROID_NDK_TOOLCHAIN_DIR/arm64-$ANDROID_NDK_API_VERSION  || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="arm64" --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/arm64-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
test -d $ANDROID_NDK_TOOLCHAIN_DIR/x86-$ANDROID_NDK_API_VERSION    || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="x86"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/x86-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
test -d $ANDROID_NDK_TOOLCHAIN_DIR/x86_64-$ANDROID_NDK_API_VERSION || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="x86_64"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/x86_64-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
'''


@for_run_task.add
def _for_run_task(config, tasks):
    for task in tasks:
        command = [
            "/bin/bash",
            "-c",
            "cat <<'SCRIPT' > ../script.sh && bash -e ../script.sh\n"
            "export TERM=dumb\n{}\nSCRIPT".format(SCRIPT)
        ]

        task["run"]["command"] = command
        yield task
