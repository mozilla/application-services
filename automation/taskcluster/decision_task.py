# coding: utf8

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import os.path
from decisionlib import *

def main(task_for, mock=False):
    android_libs_task = android_libs()
    desktop_linux_libs_task = desktop_linux_libs()
    desktop_macos_libs_task = desktop_macos_libs()
    desktop_win32_x86_64_libs_task = desktop_win32_x86_64_libs()

    if (task_for == "github-pull-request") or (task_for == "github-push"):
        android_multiarch(android_libs_task, desktop_linux_libs_task, desktop_macos_libs_task, desktop_win32_x86_64_libs_task)
    elif task_for == "github-release":
        android_multiarch_release(android_libs_task, desktop_linux_libs_task, desktop_macos_libs_task, desktop_win32_x86_64_libs_task)
    else:  # pragma: no cover
        raise ValueError("Unrecognized $TASK_FOR value: %r", task_for)


build_artifacts_expire_in = "1 month"
build_dependencies_artifacts_expire_in = "3 month"
log_artifacts_expire_in = "1 year"

build_env = {
    "RUST_BACKTRACE": "1",
    "RUSTFLAGS": "-Dwarnings",
    "CARGO_INCREMENTAL": "0",
    "CI": "1",
}
linux_build_env = {
    "TERM": "dumb",  # Keep Gradle output sensible.
    "CCACHE": "sccache",
    "RUSTC_WRAPPER": "sccache",
    "SCCACHE_IDLE_TIMEOUT": "1200",
    "SCCACHE_CACHE_SIZE": "40G",
    "SCCACHE_ERROR_LOG": "/build/sccache.log",
    "RUST_LOG": "sccache=info",
}


def android_libs():
    return (
        linux_build_task("Android libs (all architectures): build")
        .with_script("""
            pushd libs
            ./build-all.sh android
            popd
            tar -czf /build/repo/target.tar.gz libs/android
        """)
        .with_artifacts(
            "/build/repo/target.tar.gz",
        )
        .find_or_create("build.libs.android." + CONFIG.git_sha_for_directory("libs"))
    )

def desktop_linux_libs():
    return (
        linux_build_task("Desktop libs (Linux): build")
        .with_script("""
            pushd libs
            ./build-all.sh desktop
            popd
            tar -czf /build/repo/target.tar.gz libs/desktop
        """)
        .with_artifacts(
            "/build/repo/target.tar.gz",
        )
        .find_or_create("build.libs.desktop.linux." + CONFIG.git_sha_for_directory("libs"))
    )

def desktop_macos_libs():
    return (
        linux_target_macos_build_task("Desktop libs (macOS): build")
        .with_script("""
            pushd libs
            ./build-all.sh darwin
            popd
            tar -czf /build/repo/target.tar.gz libs/desktop
        """)
        .with_artifacts(
            "/build/repo/target.tar.gz",
        )
        .find_or_create("build.libs.desktop.macos." + CONFIG.git_sha_for_directory("libs"))
    )

def desktop_win32_x86_64_libs():
    return (
        linux_build_task("Desktop libs (win32-x86-64): build")
        .with_script("""
            apt-get install --quiet --yes --no-install-recommends mingw-w64
            pushd libs
            ./build-all.sh win32-x86-64
            popd
            tar -czf /build/repo/target.tar.gz libs/desktop
        """)
        .with_artifacts(
            "/build/repo/target.tar.gz",
        )
        .find_or_create("build.libs.desktop.win32-x86-64." + CONFIG.git_sha_for_directory("libs"))
    )

def android_multiarch(android_libs_task, desktop_linux_libs_task, desktop_macos_libs_task, desktop_win32_x86_64_libs_task):
    return (
        linux_target_macos_build_task("Android (all architectures): build and test")
        .with_curl_artifact_script(android_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_linux_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_macos_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_win32_x86_64_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_script("""
            yes | sdkmanager --update
            yes | sdkmanager --licenses
            ./gradlew --no-daemon clean
            ./gradlew --no-daemon testDebug
            ./gradlew --no-daemon assembleRelease
            ./gradlew --no-daemon publish :zipMavenArtifacts
        """)
        .with_artifacts("/build/repo/build/target.maven.zip")
        .create()
    )

def android_multiarch_release(android_libs_task, desktop_linux_libs_task, desktop_macos_libs_task, desktop_win32_x86_64_libs_task):
    return (
        linux_target_macos_build_task("Android (all architectures): build and test and release")
        .with_curl_artifact_script(android_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_linux_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_macos_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_win32_x86_64_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_script("""
            yes | sdkmanager --update
            yes | sdkmanager --licenses
            ./gradlew --no-daemon clean
            ./gradlew --no-daemon testDebug
            ./gradlew --no-daemon assembleRelease
            ./gradlew --no-daemon publish :zipMavenArtifacts
            python automation/taskcluster/release/fetch-bintray-api-key.py
            ./gradlew bintrayUpload --debug -PvcsTag="${GIT_SHA}"
        """)
        .with_artifacts("/build/repo/build/target.maven.zip")
        .with_scopes("secrets:get:project/application-services/publish")
        .with_features("taskclusterProxy")
        .create()
        # Eventually we can index these releases, if we choose to.
        # .find_or_create("build.android_release." + CONFIG.git_sha)
    )


def dockerfile_path(name):
    return os.path.join(os.path.dirname(__file__), "docker", name + ".dockerfile")


def linux_task(name):
    return DockerWorkerTask(name).with_worker_type("application-services-r")


def linux_build_task(name):
    return (
        linux_task(name)
        # https://docs.taskcluster.net/docs/reference/workers/docker-worker/docs/caches
        .with_scopes("docker-worker:cache:application-services-*")
        .with_caches(**{
            "application-services-cargo-registry": "/root/.cargo/registry",
            "application-services-cargo-git": "/root/.cargo/git",
            "application-services-sccache": "/root/.cache/sccache",
            "application-services-gradle": "/root/.gradle",
            "application-services-rustup": "/root/.rustup",
            "application-services-android-ndk-toolchain": "/root/.android-ndk-r15c-toolchain",
        })
        .with_index_and_artifacts_expire_in(build_artifacts_expire_in)
        .with_artifacts("/build/sccache.log")
        .with_max_run_time_minutes(120)
        .with_dockerfile(dockerfile_path("build"))
        .with_env(**build_env, **linux_build_env)
        .with_script("""
            rustup toolchain install 1.33.0
            rustup default 1.33.0
            # rustup target add x86_64-unknown-linux-gnu # See https://github.com/rust-lang-nursery/rustup.rs/issues/1533.

            rustup target add x86_64-linux-android
            rustup target add i686-linux-android
            rustup target add armv7-linux-androideabi
            rustup target add aarch64-linux-android
        """)
        .with_script("""
            test -d $ANDROID_NDK_TOOLCHAIN_DIR/arm-$ANDROID_NDK_API_VERSION    || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="arm"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/arm-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
            test -d $ANDROID_NDK_TOOLCHAIN_DIR/arm64-$ANDROID_NDK_API_VERSION  || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="arm64" --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/arm64-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
            test -d $ANDROID_NDK_TOOLCHAIN_DIR/x86-$ANDROID_NDK_API_VERSION    || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="x86"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/x86-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
            test -d $ANDROID_NDK_TOOLCHAIN_DIR/x86_64-$ANDROID_NDK_API_VERSION || $ANDROID_NDK_ROOT/build/tools/make_standalone_toolchain.py --arch="x86_64"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/x86_64-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
        """)
        .with_repo()
        .with_script("""
            ./libs/verify-android-environment.sh
        """)
    )

def linux_target_macos_build_task(name):
    return (
        linux_build_task(name)
        .with_scopes('docker-worker:relengapi-proxy:tooltool.download.internal')
        .with_features('relengAPIProxy')
        .with_script("""
            rustup target add x86_64-apple-darwin

            pushd libs
            ./cross-compile-macos-on-linux-desktop-libs.sh
            popd

            # Rust requires dsymutil on the PATH: https://github.com/rust-lang/rust/issues/52728.
            export PATH=$PATH:/tmp/clang/bin

            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_SQLCIPHER_LIB_DIR=/build/repo/libs/desktop/darwin/sqlcipher/lib
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_OPENSSL_DIR=/build/repo/libs/desktop/darwin/openssl
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_CC=/tmp/clang/bin/clang
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_TOOLCHAIN_PREFIX=/tmp/cctools/bin
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_AR=/tmp/cctools/bin/x86_64-apple-darwin11-ar
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_RANLIB=/tmp/cctools/bin/x86_64-apple-darwin11-ranlib
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_LD_LIBRARY_PATH=/tmp/clang/lib
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS="-C linker=/tmp/clang/bin/clang -C link-arg=-B -C link-arg=/tmp/cctools/bin -C link-arg=-target -C link-arg=x86_64-apple-darwin11 -C link-arg=-isysroot -C link-arg=/tmp/MacOSX10.11.sdk -C link-arg=-Wl,-syslibroot,/tmp/MacOSX10.11.sdk -C link-arg=-Wl,-dead_strip"
            # For ring's use of `cc`.
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_CFLAGS_x86_64_apple_darwin="-B /tmp/cctools/bin -target x86_64-apple-darwin11 -isysroot /tmp/MacOSX10.11.sdk -Wl,-syslibroot,/tmp/MacOSX10.11.sdk -Wl,-dead_strip"

            apt-get install --quiet --yes --no-install-recommends mingw-w64
            rustup target add x86_64-pc-windows-gnu
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C linker=x86_64-w64-mingw32-gcc"
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-ar
            export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_PC_WINDOWS_GNU_CC=x86_64-w64-mingw32-gcc
        """)
    )

CONFIG.task_name_template = "Application Services: %s"
CONFIG.index_prefix = "project.application-services.application-services"
CONFIG.docker_image_build_worker_type = "application-services-r"
CONFIG.docker_images_expire_in = build_dependencies_artifacts_expire_in
CONFIG.repacked_msi_files_expire_in = build_dependencies_artifacts_expire_in


if __name__ == "__main__":  # pragma: no cover
    main(task_for=os.environ["TASK_FOR"])
