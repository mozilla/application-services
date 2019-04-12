#!/usr/bin/env bash

# This script sets up the necessary toolchains to build for Android.

set -eu

# Keep these 3 in sync.
TARGET_ARCHS=("x86_64" "x86" "arm64" "arm")
RUST_TARGETS=("x86_64-linux-android" "i686-linux-android" "aarch64-linux-android" "armv7-linux-androideabi")
CLANG_BINS=("x86_64-linux-android-clang" "i686-linux-android-clang" "aarch64-linux-android-clang" "arm-linux-androideabi-clang")

ANDROID_NDK_ROOT="${1:-${ANDROID_NDK_ROOT}}"

if [ -z "${ANDROID_NDK_ROOT}" ]; then
    echo "Usage:"
    echo "./setup_toolchains_local.sh <ANDROID_NDK_ROOT>"
    exit 1
fi

source "$(dirname "${0}")/android_defaults.sh"
mkdir -p ${ANDROID_NDK_TOOLCHAIN_DIR}
echo "Installing toolchains for the following architectures: ${TARGET_ARCHS[@]}."
echo "The toolchains will be installed in ${ANDROID_NDK_TOOLCHAIN_DIR} (ANDROID_NDK_TOOLCHAIN_DIR)."
echo "The Android API version is set to ${ANDROID_NDK_API_VERSION} (ANDROID_NDK_API_VERSION)."
echo ""

# Toolchains installation
for ARCH in "${TARGET_ARCHS[@]}"; do
  if [ ! -d "${ANDROID_NDK_TOOLCHAIN_DIR}/${ARCH}-${ANDROID_NDK_API_VERSION}" ]; then
    echo "Installing ${ARCH} toolchain..."
    python "${ANDROID_NDK_ROOT}/build/tools/make_standalone_toolchain.py" --arch="${ARCH}" --api="${ANDROID_NDK_API_VERSION}" --install-dir="${ANDROID_NDK_TOOLCHAIN_DIR}/${ARCH}-${ANDROID_NDK_API_VERSION}" --deprecated-headers --force
  else
    echo "${ARCH} toolchain already exists. Skipping installation."
  fi
done

# Setup cargo linkers
CONFIG_FILE="$(dirname "${0}")/../.cargo/config"
read -p "Would you like to set-up the toolchain linkers in .cargo/config? (you should say yes) " -n 1 -r
echo ""
if [[ ! ${REPLY} =~ ^[Yy]$ ]]
then
  exit 0
fi

mkdir -p "$(dirname "${0}")/../.cargo"
echo -n "" > ${CONFIG_FILE} # Clear the file first
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[${i}]}
  RUST_TARGET=${RUST_TARGETS[${i}]}
  CLANG_BIN=${CLANG_BINS[${i}]}
  echo "[target.${RUST_TARGET}]
linker = \"${ANDROID_NDK_TOOLCHAIN_DIR}/${ARCH}-${ANDROID_NDK_API_VERSION}/bin/${CLANG_BIN}\"
" >> ${CONFIG_FILE}
done
echo "Cargo config written to ${CONFIG_FILE}."
