#!/bin/bash

# This script downloads and builds the iOS cjose library.

set -e

if [ "$#" -ne 7 ]
then
    echo "Usage:"
    echo "./build-cjose-ios.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <HOST> <IOS_MIN_SDK_VERSION> <JANSSON_DIR> <OPENSSL_DIR>"
    exit 1
fi

CJOSEDIR=$1
DIST_DIR=$2
ARCH=$3
HOST=$4
IOS_MIN_SDK_VERSION=$5
JANSSON_DIR=$6
OPENSSL_DIR=$7

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

cd "${CJOSEDIR}"

if [[ "${ARCH}" == "i386" || "${ARCH}" == "x86_64" ]]; then
  PLATFORM="iPhoneSimulator"
else
  PLATFORM="iPhoneOS"
fi
DEVELOPER=$(xcode-select -print-path)
SYSROOT=${DEVELOPER}/Platforms/${PLATFORM}.platform/Developer/SDKs/${PLATFORM}.sdk

export CFLAGS="-arch ${ARCH} -Os -isysroot ${SYSROOT} -miphoneos-version-min=${IOS_MIN_SDK_VERSION} -fembed-bitcode"
export LDFLAGS="-arch ${ARCH} -isysroot ${SYSROOT}"
make clean || true
./configure --host="$HOST" --with-openssl="$OPENSSL_DIR" --with-jansson="$JANSSON_DIR" && make
mkdir -p "$DIST_DIR""/include"
mkdir -p "$DIST_DIR""/lib"
cp -p src/.libs/libcjose.a "$DIST_DIR""/lib"
cp "$PWD"/include/cjose/*.h "$DIST_DIR""/include"
