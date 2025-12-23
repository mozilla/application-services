#!/usr/bin/env bash

# This script cross-compiles NSS and NSPR for macOS from Linux.
# It is specifically designed for darwin cross-compilation in CI.

set -euvx

if [[ "${#}" -lt 1 ]] || [[ "${#}" -gt 2 ]]
then
  echo "Usage:"
  echo "./build-nss-macos-cross.sh <ABSOLUTE_SRC_DIR> [CROSS_COMPILE_TARGET]"
  exit 1
fi

NSS_SRC_DIR=${1}
CROSS_COMPILE_TARGET=${2:-darwin-aarch64}

# Set architecture-specific variables based on target
if [[ "${CROSS_COMPILE_TARGET}" == "darwin-aarch64" ]]; then
  DIST_DIR=$(abspath "desktop/darwin-aarch64/nss")
  NSS_TARGET="aarch64-apple-darwin"
  GYP_ARCH="arm64"
elif [[ "${CROSS_COMPILE_TARGET}" == "darwin-x86-64" ]]; then
  DIST_DIR=$(abspath "desktop/darwin-x86-64/nss")
  NSS_TARGET="x86_64-apple-darwin"
  GYP_ARCH="x64"
else
  echo "Unsupported cross-compile target: ${CROSS_COMPILE_TARGET}"
  exit 1
fi

if [[ -d "${DIST_DIR}" ]]; then
  echo "${DIST_DIR} folder already exists. Skipping build."
  exit 0
fi

# Read toolchain configuration from ORG_GRADLE_PROJECT environment variables
# These are set by cross-compile-setup.sh in CI
RUST_ANDROID_PREFIX=$(echo "ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_${NSS_TARGET}" | tr '[:lower:]-' '[:upper:]_')

# Check that NSS_DIR is set to detect CI environment
nss_dir_var="${RUST_ANDROID_PREFIX}_NSS_DIR"
if [[ -z "${!nss_dir_var}" ]]; then
  echo "Error: ${nss_dir_var} is not set"
  echo "This script must be run in a CI environment with cross-compile-setup.sh sourced"
  exit 1
fi

# Use toolchain configuration from environment
eval "CC=\$${RUST_ANDROID_PREFIX}_CC"
eval "AR=\$${RUST_ANDROID_PREFIX}_AR"
eval "RANLIB=\$${RUST_ANDROID_PREFIX}_RANLIB"
eval "STRIP=\$${RUST_ANDROID_PREFIX}_TOOLCHAIN_PREFIX/${NSS_TARGET}-strip"
eval "CFLAGS=\$${RUST_ANDROID_PREFIX}_CFLAGS_${NSS_TARGET//-/_}"
eval "LDFLAGS=\$${RUST_ANDROID_PREFIX}_LDFLAGS_${NSS_TARGET//-/_}"

# Build NSPR
NSPR_BUILD_DIR=$(mktemp -d)
pushd "${NSPR_BUILD_DIR}"
"${NSS_SRC_DIR}"/nspr/configure \
  STRIP="${STRIP}" \
  RANLIB="${RANLIB}" \
  AR="${AR}" \
  AS="${AS:-${AR}}" \
  LD="${LD:-${AR}}" \
  CC="${CC}" \
  CCC="${CC}" \
  CFLAGS="${CFLAGS}" \
  LDFLAGS="${LDFLAGS}" \
  --target="${NSS_TARGET}" \
  --enable-64bit \
  --disable-debug \
  --enable-optimize
make
popd

# Build NSS using gyp
NSS_BUILD_DIR=$(mktemp -d)
rm -rf "${NSS_SRC_DIR}/nss/out"

gyp -f ninja "${NSS_SRC_DIR}/nss/nss.gyp" \
  --depth "${NSS_SRC_DIR}/nss/" \
  --generator-output=. \
  -DOS=mac \
  -Dnspr_lib_dir="${NSPR_BUILD_DIR}/dist/lib" \
  -Dnspr_include_dir="${NSPR_BUILD_DIR}/dist/include/nspr" \
  -Dnss_dist_dir="${NSS_BUILD_DIR}" \
  -Dnss_dist_obj_dir="${NSS_BUILD_DIR}" \
  -Dhost_arch="${GYP_ARCH}" \
  -Dtarget_arch="${GYP_ARCH}" \
  -Dstatic_libs=1 \
  -Ddisable_dbm=1 \
  -Dsign_libs=0 \
  -Denable_sslkeylogfile=0 \
  -Ddisable_tests=1 \
  -Ddisable_libpkix=1 \
  -Dpython=python3

GENERATED_DIR="${NSS_SRC_DIR}/nss/out/Release/"
ninja -C "${GENERATED_DIR}"

# Assemble the DIST_DIR with relevant libraries and headers
./copy-nss-libs.sh \
  "mac" \
  "${GYP_ARCH}" \
  "${DIST_DIR}" \
  "${NSS_BUILD_DIR}/lib" \
  "${NSPR_BUILD_DIR}/dist/lib" \
  "${NSS_BUILD_DIR}/public/nss" \
  "${NSPR_BUILD_DIR}/dist/include/nspr"

# Cleanup
rm -rf "${NSS_BUILD_DIR}"
rm -rf "${NSPR_BUILD_DIR}"
