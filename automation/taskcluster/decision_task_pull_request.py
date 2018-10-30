# coding: utf8

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import os.path
from decisionlib import *


def main(task_for, mock=False):
    if task_for == "github-pull-request":
        # linux_tidy_unit()
        android_libs_task = android_libs()
        desktop_linux_libs_task = desktop_linux_libs()
        android_arm32(android_libs_task)
        # if mock:
        #     linux_wpt()
        #     linux_build_task("Indexed by task definition").find_or_create()

    else:  # pragma: no cover
        raise ValueError("Unrecognized $TASK_FOR value: %r", task_for)


build_artifacts_expire_in = "1 month"
build_dependencies_artifacts_expire_in = "3 month"
log_artifacts_expire_in = "1 year"

build_env = {
    "RUST_BACKTRACE": "1",
    "RUSTFLAGS": "-Dwarnings",
    "CARGO_INCREMENTAL": "0",
}
linux_build_env = {
    "CCACHE": "sccache",
    "RUSTC_WRAPPER": "sccache",
    "SCCACHE_IDLE_TIMEOUT": "1200",
    # "SHELL": "/bin/dash",  # For SpiderMonkey’s build system
}


# def linux_tidy_unit():
#     return linux_build_task("Linux x64: tidy + dev build + unit tests").with_script("""
#         ./mach test-tidy --no-progress --all
#         ./mach build --dev
#         ./mach test-unit
#         ./mach package --dev
#         ./mach test-tidy --no-progress --self-test
#         ./etc/memory_reports_over_time.py --test
#         ./etc/taskcluster/mock.py
#         ./etc/ci/lockfile_changed.sh
#         ./etc/ci/check_no_panic.sh
#     """).create()


# def with_rust_nightly():
#     return linux_build_task("Linux x64: with Rust Nightly").with_script("""
#         echo "nightly" > rust-toolchain
#         ./mach build --dev
#         ./mach test-unit
#     """).create()


# TODO: work through /repo vs /build/repo

def android_libs():
    return (
        linux_build_task("Android libs (all architectures): build")
        .with_script("""
            ./scripts/taskcluster-android.sh
            tar -czf /build/repo/target.tar.gz libs/android
        """)
        # XXX names change: public/bin/mozilla/XXX to public/XXX
        .with_artifacts(
            "/build/repo/target.tar.gz",
        )
        .find_or_create("build.libs.android." + CONFIG.git_sha_for_directory("libs"))
    )

def desktop_linux_libs():
    return (
        linux_build_task("Desktop libs (Linux): build")
        .with_script("""
            pushd libs && ./build-all.sh desktop && popd
            tar -czf /build/repo/target.tar.gz libs/desktop
        """)
        # XXX names change: public/bin/mozilla/XXX to public/XXX
        .with_artifacts(
            "/build/repo/target.tar.gz",
        )
        .find_or_create("build.libs.desktop.linux." + CONFIG.git_sha_for_directory("libs"))
    )

def android_arm32(build_task):
    return (
        linux_build_task("Android (all architectures): build")
        .with_env(BUILD_TASK_ID=build_task)
        .with_env(SCCACHE_CACHE_SIZE='40G')
        .with_env(RUST_LOG='sccache=info')
        .with_dependencies(build_task)
        .with_script("""
            curl --silent --show-error --fail --location --retry 5 --retry-delay 10 https://github.com/mozilla/sccache/releases/download/0.2.7/sccache-0.2.7-x86_64-unknown-linux-musl.tar.gz | tar -xz --strip-components=1 -C /usr/local/bin/ sccache-0.2.7-x86_64-unknown-linux-musl/sccache
            ./automation/taskcluster/curl-artifact.sh ${BUILD_TASK_ID} target.tar.gz | tar -xz
            ./gradlew --no-daemon clean :fxa-client-library:assembleRelease :logins-library:assembleRelease
        """)
        # XXX names change: public/bin/mozilla/XXX to public/XXX
        .with_artifacts(
            "/build/repo/fxa-client/sdks/android/library/build/outputs/aar/fxaclient-release.aar",
            "/build/repo/logins-api/android/library/build/outputs/aar/logins-release.aar",
        )
        .create()
        # .find_or_create("build.android_armv7_release." + CONFIG.git_sha)
    )


# def linux_wpt():
#     release_build_task = linux_release_build()
#     total_chunks = 2
#     for i in range(total_chunks):
#         this_chunk = i + 1
#         wpt_chunk(release_build_task, total_chunks, this_chunk)


# def linux_release_build():
#     return (
#         linux_build_task("Linux x64: release build")
#         .with_script("""
#             ./mach build --release --with-debug-assertions -p servo
#             ./etc/ci/lockfile_changed.sh
#             tar -czf /target.tar.gz \
#                 target/release/servo \
#                 target/release/build/osmesa-src-*/output \
#                 target/release/build/osmesa-src-*/out/lib/gallium
#         """)
#         .with_artifacts("/target.tar.gz")
#         # .find_or_create("build.linux_x64_release." + CONFIG.git_sha)
#     )


# def wpt_chunk(release_build_task, total_chunks, this_chunk):
#     name = "Linux x64: WPT chunk %s / %s" % (this_chunk, total_chunks)
#     script = """
#         ./mach test-wpt \
#             --release \
#             --processes 24 \
#             --total-chunks "$TOTAL_CHUNKS" \
#             --this-chunk "$THIS_CHUNK" \
#             --log-raw test-wpt.log \
#             --log-errorsummary wpt-errorsummary.log \
#             --always-succeed
#         ./mach filter-intermittents\
#             wpt-errorsummary.log \
#             --log-intermittents intermittents.log \
#             --log-filteredsummary filtered-wpt-errorsummary.log \
#             --tracker-api default
#     """
#     # FIXME: --reporter-api default
#     # IndexError: list index out of range
#     # File "/build/repo/python/servo/testing_commands.py", line 533, in filter_intermittents
#     #   pull_request = int(last_merge.split(' ')[4][1:])
#     if this_chunk == 1:
#         name += " + extra"
#         script += """
#             ./mach test-wpt-failure
#             ./mach test-wpt --release --binary-arg=--multiprocess --processes 24 \
#                 --log-raw test-wpt-mp.log \
#                 --log-errorsummary wpt-mp-errorsummary.log \
#                 eventsource
#         """
#     return (
#         linux_run_task(name, release_build_task, script)
#         .with_env(TOTAL_CHUNKS=total_chunks, THIS_CHUNK=this_chunk)
#         .create()
#     )


# def linux_run_task(name, build_task, script):
#     return (
#         linux_task(name)
#         .with_dockerfile(dockerfile_path("run"))
#         .with_early_script("""
#             ./etc/taskcluster/curl-artifact.sh ${BUILD_TASK_ID} target.tar.gz | tar -xz
#         """)
#         .with_env(BUILD_TASK_ID=build_task)
#         .with_dependencies(build_task)
#         .with_script(script)
#         .with_index_and_artifacts_expire_in(log_artifacts_expire_in)
#         .with_artifacts(*[
#             "/build/repo/" + word
#             for word in script.split() if word.endswith(".log")
#         ])
#         .with_max_run_time_minutes(60)
#     )


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
            # "servo-cargo-registry": "/root/.cargo/registry",
            # "servo-cargo-git": "/root/.cargo/git",
            # "servo-rustup": "/root/.rustup",
            "application-services-sccache": "/root/.cache/sccache",
            # "servo-gradle": "/root/.gradle",
        })
        .with_index_and_artifacts_expire_in(build_artifacts_expire_in)
        .with_max_run_time_minutes(60)
        .with_docker_image(
            'mozillamobile/rust-component:buildtools-27.0.3-ndk-r15c-ndk-version-21-rust-stable-1.28.0-rust-beta-1.29.0-beta.15'
        )
        # .with_dockerfile(dockerfile_path("build"))
        .with_env(**build_env, **linux_build_env)
        .with_repo()
        .with_index_and_artifacts_expire_in(build_artifacts_expire_in)
    )


CONFIG.task_name_template = "Application Services: %s"
CONFIG.index_prefix = "project.application-services.application-services"
CONFIG.docker_images_expire_in = build_dependencies_artifacts_expire_in
CONFIG.repacked_msi_files_expire_in = build_dependencies_artifacts_expire_in


if __name__ == "__main__":  # pragma: no cover
    main(task_for=os.environ["TASK_FOR"])
