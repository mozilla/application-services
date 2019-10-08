# %ARG DOCKER_IMAGE_PARENT
FROM $DOCKER_IMAGE_PARENT

MAINTAINER Edouard Oger "eoger@mozilla.com"

rustup target add x86_64-apple-darwin

#!/usr/bin/env bash

# Install clang, a port of cctools, and the macOS SDK into /tmp. This
# is all cribbed from mozilla-central; start at
# https://searchfox.org/mozilla-central/rev/39cb1e96cf97713c444c5a0404d4f84627aee85d/build/macosx/cross-mozconfig.common.

# TODO the macos tools shouldn't be in the docker image
# https://mozilla.slack.com/archives/GEZ4ZJCRM/p1570562744025800
COPY macos-cc-tools.manifest /tmp/

WORKDIR /tmp

RUN tooltool.py \
  --url=http://taskcluster/tooltool.mozilla-releng.net/ \
  --manifest="/tmp/macos-cc-tools.manifest" \
  fetch


# Rust requires dsymutil on the PATH: https://github.com/rust-lang/rust/issues/52728.
ENV PATH=$PATH:/tmp/clang/bin
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_NSS_STATIC=1
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_NSS_DIR=/build/repo/libs/desktop/darwin/nss
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_SQLCIPHER_LIB_DIR=/build/repo/libs/desktop/darwin/sqlcipher/lib
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_CC=/tmp/clang/bin/clang
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_TOOLCHAIN_PREFIX=/tmp/cctools/bin
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_AR=/tmp/cctools/bin/x86_64-darwin11-ar
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_RANLIB=/tmp/cctools/bin/x86_64-darwin11-ranlib
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_LD_LIBRARY_PATH=/tmp/clang/lib
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS="-C linker=/tmp/clang/bin/clang -C link-arg=-B -C link-arg=/tmp/cctools/bin -C link-arg=-target -C link-arg=x86_64-darwin11 -C link-arg=-isysroot -C link-arg=/tmp/MacOSX10.11.sdk -C link-arg=-Wl,-syslibroot,/tmp/MacOSX10.11.sdk -C link-arg=-Wl,-dead_strip"
    # For ring's use of `cc`.
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_APPLE_DARWIN_CFLAGS_x86_64_apple_darwin="-B /tmp/cctools/bin -target x86_64-darwin11 -isysroot /tmp/MacOSX10.11.sdk -Wl,-syslibroot,/tmp/MacOSX10.11.sdk -Wl,-dead_strip"
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C linker=x86_64-w64-mingw32-gcc"
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-ar
ENV ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_PC_WINDOWS_GNU_CC=x86_64-w64-mingw32-gcc

RUN rustup target add x86_64-pc-windows-gnu

# taskgraph's `check_caches_are_volumes(...)` function doesn't take docker inheritance into account, so re-declare
# these VOLUMEs from `linux/Dockerfile`.
VOLUME /builds/worker/checkouts
VOLUME /builds/worker/.cache
