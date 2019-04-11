#!/usr/bin/env bash

# This script downloads and builds the iOS openssl library.

set -euvx

if [ "${#}" -ne 4 ]
then
    echo "Usage:"
    echo "./build-openssl-ios.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <IOS_MIN_SDK_VERSION>"
    exit 1
fi

OPENSSL_DIR=${1}
DIST_DIR=${2}
ARCH=${3}
IOS_MIN_SDK_VERSION=${4}

if [ -d "${DIST_DIR}" ]; then
  echo "${DIST_DIR} folder already exists. Skipping build."
  exit 0
fi

cd "${OPENSSL_DIR}"

OPENSSL_OUTPUT_PATH="/tmp/openssl-${ARCH}_${$}"
mkdir -p "${OPENSSL_OUTPUT_PATH}"

if [[ "${ARCH}" == "x86_64" ]]; then
  OS_COMPILER="iPhoneSimulator"
  HOST="darwin64-x86_64-cc"
elif [[ "${ARCH}" == "arm64" ]]; then
  OS_COMPILER="iPhoneOS"
  HOST="ios64-cross"
else
  echo "Unsupported architecture"
  exit 1
fi

DEVELOPER=$(xcode-select -print-path)
export CROSS_TOP="${DEVELOPER}/Platforms/${OS_COMPILER}.platform/Developer"
export CROSS_SDK="${OS_COMPILER}.sdk"
export CROSS_COMPILE="${DEVELOPER}/Toolchains/XcodeDefault.xctoolchain/usr/bin/"

make clean || true
./Configure ${HOST} "-arch ${ARCH} -fembed-bitcode" no-asm no-ssl3 no-comp no-hw no-engine no-async --prefix="${OPENSSL_OUTPUT_PATH}" || exit 1
if [[ "${OS_COMPILER}" == "iPhoneSimulator" ]]; then
  sed -ie "s!^CFLAGS=!CFLAGS=-isysroot ${CROSS_TOP}/SDKs/${CROSS_SDK} -mios-version-min=${IOS_MIN_SDK_VERSION} !" "Makefile"
fi
make -j6
make install_sw
mkdir -p "${DIST_DIR}/include/openssl"
mkdir -p "${DIST_DIR}/lib"
cp -p "${OPENSSL_OUTPUT_PATH}"/lib/libssl.a "${DIST_DIR}/lib"
cp -p "${OPENSSL_OUTPUT_PATH}"/lib/libcrypto.a "${DIST_DIR}/lib"
cp -L "${PWD}"/include/openssl/*.h "${DIST_DIR}/include/openssl"
rm -rf "${OPENSSL_OUTPUT_PATH}"
