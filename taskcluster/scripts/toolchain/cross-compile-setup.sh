#!/bin/bash

set -eux

SYSROOT="/tmp/MacOSX11.0.sdk"
CHECKOUT="/builds/worker/checkouts/vcs"
CLANG_BIN="/builds/worker/clang/bin"
CLANG_LIB="/builds/worker/clang/lib"
CCTOOL_BIN="/builds/worker/cctools/bin"

export PATH=$PATH:$CLANG_BIN

# Setup environment variables for rust-android-gradle plugin.
# shellcheck disable=SC2086
for TARGET in x86_64-apple-darwin aarch64-apple-darwin; do
  case "$TARGET" in
    x86_64-apple-darwin)
      BUILD_PATH="darwin-x86-64"
      ;;
    aarch64-apple-darwin)
      BUILD_PATH="darwin-aarch64"
      ;;
  esac

  RUST_ANDROID_PREFIX=$(echo "ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_${TARGET}" | tr '[:lower:]-' '[:upper:]_')

  export ${RUST_ANDROID_PREFIX}_NSS_STATIC=1
  export ${RUST_ANDROID_PREFIX}_NSS_DIR=${CHECKOUT}/libs/desktop/darwin-${BUILD_PATH}/nss
  export ${RUST_ANDROID_PREFIX}_CC=${CLANG_BIN}/clang-20
  export ${RUST_ANDROID_PREFIX}_TOOLCHAIN_PREFIX=${CCTOOL_BIN}
  export ${RUST_ANDROID_PREFIX}_AR=${CCTOOL_BIN}/${TARGET}-ar
  export ${RUST_ANDROID_PREFIX}_AS=${CCTOOL_BIN}/${TARGET}-as
  export ${RUST_ANDROID_PREFIX}_RANLIB=${CCTOOL_BIN}/${TARGET}-ranlib
  export ${RUST_ANDROID_PREFIX}_LD=${CCTOOL_BIN}/${TARGET}-ld
  export ${RUST_ANDROID_PREFIX}_LD_LIBRARY_PATH=${CLANG_LIB}
  export ${RUST_ANDROID_PREFIX}_RUSTFLAGS="-C linker=${CLANG_BIN}/clang-20 -C link-arg=-fuse-ld=${CCTOOL_BIN}/${TARGET}-ld -C link-arg=-B -C link-arg=${CCTOOL_BIN} -C link-arg=-target -C link-arg=${TARGET} -C link-arg=-isysroot -C link-arg=${SYSROOT} -C link-arg=-Wl,-syslibroot,${SYSROOT} -C link-arg=-Wl,-dead_strip"
  export ${RUST_ANDROID_PREFIX}_CFLAGS_${TARGET//-/_}="-B ${CCTOOL_BIN} -target ${TARGET} -isysroot ${SYSROOT} -fuse-ld=${CCTOOL_BIN}/${TARGET}-ld"
  export ${RUST_ANDROID_PREFIX}_BINDGEN_EXTRA_CLANG_ARGS="--sysroot ${SYSROOT}"
done

# x86_64 Windows
# The wrong linker gets used otherwise: https://github.com/rust-lang/rust/issues/33465.
export ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C linker=x86_64-w64-mingw32-gcc"

# Ensure we're compiling dependencies in non-debug mode.
# This is required for rkv/lmdb to work correctly on Android targets and not link to unavailable symbols.
export TARGET_CFLAGS="-DNDEBUG"

# Install clang, a port of cctools, and the macOS SDK into.
# cribbed from glean https://github.com/mozilla/glean/blob/b5faa56305a3a500e49fc02509001c95fde32ee6/taskcluster/scripts/cross-compile-setup.sh
pushd /builds/worker
curl -sfSL --retry 5 --retry-delay 10 \
    https://firefox-ci-tc.services.mozilla.com/api/index/v1/task/gecko.cache.level-3.toolchains.v3.linux64-cctools-port.latest/artifacts/public%2Fbuild%2Fcctools.tar.zst \
    -o cctools.tar.zst
unzstd cctools.tar.zst
tar -xf cctools.tar
rm cctools.tar.zst
curl -sfSL --retry 5 --retry-delay 10 \
    https://firefox-ci-tc.services.mozilla.com/api/index/v1/task/gecko.cache.level-3.toolchains.v3.clang-dist-toolchain.latest/artifacts/public%2Fbuild%2Fclang-dist-toolchain.tar.xz \
    -o clang-dist-toolchain.tar.xz
tar -xf clang-dist-toolchain.tar.xz
mv builds/worker/toolchains/clang clang
rm clang-dist-toolchain.tar.xz

popd

pushd /tmp || exit

tooltool.py \
  --url=http://taskcluster/tooltool.mozilla-releng.net/ \
  --manifest="${CHECKOUT}/libs/macos-cc-tools.manifest" \
  fetch
# tooltool doesn't know how to unpack zstd-files,
# so we do it manually.
tar -I zstd -xf "MacOSX11.0.sdk.tar.zst"

popd || exit

set +eu
