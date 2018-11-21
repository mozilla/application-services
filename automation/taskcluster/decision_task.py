# coding: utf8

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import json
import os.path
from decisionlib import *


def populate_chain_of_trust_required_but_unused_files():
    # Thoses files are needed to keep chainOfTrust happy. However, they have no
    # need for android-components, at the moment. For more details, see:
    # https://github.com/mozilla-releng/scriptworker/pull/209/files#r184180585

    for file_names in ('/build/repo/actions.json', '/build/repo/parameters.yml'):
        with open(file_names, 'w') as f:
            json.dump({}, f)    # Yaml is a super-set of JSON.


def main(task_for, mock=False):
    if task_for == "github-pull-request":
        # Pull request.
        android_libs_task = android_libs()
        desktop_linux_libs_task = desktop_linux_libs()
        desktop_macos_libs_task = desktop_macos_libs()

        android_arm32(android_libs_task, desktop_linux_libs_task)

    elif task_for == "github-push":
        # Push to master or a tag.
        android_libs_task = android_libs()
        desktop_linux_libs_task = desktop_linux_libs()
        desktop_macos_libs_task = desktop_macos_libs()

        if CONFIG.git_ref.startswith('refs/tags/'):
            # A release.
            build_task = android_arm32_release(android_libs_task, desktop_linux_libs_task)

            task_graph = {}
            task_graph[build_task] = {}
            task_graph[build_task]["task"] = SHARED.queue_service.task(build_task)

            print(json.dumps(task_graph, indent=4, separators=(',', ': ')))

            task_graph_path = '/build/repo/task-graph.json'
            with open(task_graph_path, 'w') as f:
                json.dump(task_graph, f)

            populate_chain_of_trust_required_but_unused_files()

        else:
            # A regular push to master.
            android_arm32(android_libs_task, desktop_linux_libs_task)

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
            ./scripts/taskcluster-android.sh
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
        linux_build_task("Desktop libs (macOS): build")
        .with_script("""
            pushd libs
            ./cross-compile-macos-on-linux-desktop-libs.sh
            ./build-all.sh osx-cross
            popd
            tar -czf /build/repo/target.tar.gz libs/desktop
        """)
        .with_scopes('docker-worker:relengapi-proxy:tooltool.download.internal')
        .with_features('relengAPIProxy')
        .with_artifacts(
            "/build/repo/target.tar.gz",
        )
        .find_or_create("build.libs.desktop.macos." + CONFIG.git_sha_for_directory("libs"))
    )

def android_arm32(android_libs_task, desktop_libs_task):
    return (
        linux_build_task("Android (all architectures): build and test")
        .with_curl_artifact_script(android_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_script("""
            ./gradlew --no-daemon clean
            ./gradlew --no-daemon :fxa-client-library:testDebug :logins-library:testDebug :places-library:testDebug
            ./gradlew --no-daemon :fxa-client-library:assembleRelease :logins-library:assembleRelease :places-library:assembleRelease
        """)
        .with_artifacts(
            "/build/repo/fxa-client/sdks/android/library/build/outputs/aar/fxaclient-library-release.aar",
            "/build/repo/logins-api/android/library/build/outputs/aar/logins-library-release.aar",
            "/build/repo/components/places/android/library/build/outputs/aar/places-library-release.aar",
        )
        .create()
    )

def android_arm32_release(android_libs_task, desktop_libs_task):
    return (
        linux_build_task("Android (all architectures): build and test and release")
        .with_curl_artifact_script(android_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_curl_artifact_script(desktop_libs_task, "target.tar.gz")
        .with_script("tar -xzf target.tar.gz")
        .with_script("""
            ./gradlew --no-daemon clean
            ./gradlew --no-daemon :fxa-client-library:testDebug :logins-library:testDebug :places-library:testDebug
            ./gradlew --no-daemon :fxa-client-library:assembleRelease :logins-library:assembleRelease :places-library:assembleRelease
        """)
        .with_artifacts(
            "/build/repo/fxa-client/sdks/android/library/build/outputs/aar/fxaclient-library-release.aar",
            "/build/repo/logins-api/android/library/build/outputs/aar/logins-library-release.aar",
            "/build/repo/components/places/android/library/build/outputs/aar/places-library-release.aar",
        )
        # .with_scopes("secrets:get:project/application-services/publish")
        .with_features("taskclusterProxy")

        .with_features("chainOfTrust")
        .with_worker_type("gecko-focus")

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
        .with_max_run_time_minutes(60)
        .with_dockerfile(dockerfile_path("build"))
        .with_env(**build_env, **linux_build_env)
        .with_script("""
            rustup toolchain install 1.30.1
            rustup default 1.30.1
            # rustup target add x86_64-unknown-linux-gnu # See https://github.com/rust-lang-nursery/rustup.rs/issues/1533.
            rustup target add x86_64-apple-darwin

            rustup target add i686-linux-android
            rustup target add armv7-linux-androideabi
            rustup target add aarch64-linux-android
        """)
        .with_script("""
            test -d $ANDROID_NDK_TOOLCHAIN_DIR/arm-$ANDROID_NDK_API_VERSION   || $ANDROID_NDK_HOME/build/tools/make_standalone_toolchain.py --arch="arm"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/arm-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
            test -d $ANDROID_NDK_TOOLCHAIN_DIR/arm64-$ANDROID_NDK_API_VERSION || $ANDROID_NDK_HOME/build/tools/make_standalone_toolchain.py --arch="arm64" --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/arm64-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
            test -d $ANDROID_NDK_TOOLCHAIN_DIR/x86-$ANDROID_NDK_API_VERSION   || $ANDROID_NDK_HOME/build/tools/make_standalone_toolchain.py --arch="x86"   --api="$ANDROID_NDK_API_VERSION" --install-dir="$ANDROID_NDK_TOOLCHAIN_DIR/x86-$ANDROID_NDK_API_VERSION" --deprecated-headers --force
        """)
        .with_repo()
    )


CONFIG.task_name_template = "Application Services: %s"
CONFIG.index_prefix = "project.application-services.application-services"
CONFIG.docker_image_build_worker_type = "application-services-r"
CONFIG.docker_images_expire_in = build_dependencies_artifacts_expire_in
CONFIG.repacked_msi_files_expire_in = build_dependencies_artifacts_expire_in


if __name__ == "__main__":  # pragma: no cover
    main(task_for=os.environ["TASK_FOR"])
