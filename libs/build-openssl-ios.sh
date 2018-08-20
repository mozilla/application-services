#!/usr/bin/env bash

# This script downloads and builds the iOS openssl library.

set -e

if [ "$#" -ne 4 ]
then
    echo "Usage:"
    echo "./build-openssl-ios.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <IOS_MIN_SDK_VERSION>"
    exit 1
fi

OPENSSL_DIR=$1
DIST_DIR=$2
ARCH=$3
IOS_MIN_SDK_VERSION=$4

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

cd "${OPENSSL_DIR}"

OPENSSL_OUTPUT_PATH="/tmp/openssl-""$ARCH"_$$
mkdir -p "$OPENSSL_OUTPUT_PATH"

if [[ "${ARCH}" == "i386" || "${ARCH}" == "x86_64" ]]; then
  PLATFORM="iPhoneSimulator"
else
  PLATFORM="iPhoneOS"
fi

DEVELOPER=$(xcode-select -print-path)
export PLATFORM="$PLATFORM"
export CROSS_TOP="${DEVELOPER}/Platforms/${PLATFORM}.platform/Developer"
export CROSS_SDK="${PLATFORM}.sdk"
export BUILD_TOOLS="${DEVELOPER}"
export CC="${BUILD_TOOLS}/usr/bin/gcc -fembed-bitcode -arch ${ARCH}"

make clean || true
if [[ "${ARCH}" == "x86_64" ]]; then
  ./Configure no-asm darwin64-x86_64-cc --openssldir="$OPENSSL_OUTPUT_PATH"
else
  ./Configure iphoneos-cross --openssldir="$OPENSSL_OUTPUT_PATH"
fi

sed -ie "s!^CFLAG=!CFLAG=-isysroot ${CROSS_TOP}/SDKs/${CROSS_SDK} -miphoneos-version-min=${IOS_MIN_SDK_VERSION} !" "Makefile"

make -j6 && make install
mkdir -p "$DIST_DIR""/include/openssl"
mkdir -p "$DIST_DIR""/lib"
cp -p "$OPENSSL_OUTPUT_PATH"/lib/libssl.a "$DIST_DIR""/lib"
cp -p "$OPENSSL_OUTPUT_PATH"/lib/libcrypto.a "$DIST_DIR""/lib"
cp -L "$PWD"/include/openssl/*.h "${DIST_DIR}/include/openssl"
rm -rf "$OPENSSL_OUTPUT_PATH"
