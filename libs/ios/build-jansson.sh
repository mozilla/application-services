#!/bin/bash

# This script downloads and builds the iOS jansson library with Bitcode enabled.

set -e

DIST_DIR=$(pwd)/jansson
JANSSON_VERSION="2.11"
JANSSON_DIR="jansson-${JANSSON_VERSION}"
IOS_MIN_SDK_VERSION="9.0"

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

rm -rf "${JANSSON_DIR}"

if [ ! -e "${JANSSON_DIR}.tar.gz" ]; then
  echo "Downloading ${JANSSON_DIR}.tar.gz"
  curl -O "http://www.digip.org/jansson/releases/${JANSSON_DIR}.tar.gz"
else
  echo "Using ${JANSSON_DIR}.tar.gz"
fi

echo "Unpacking ${JANSSON_DIR}"
tar xfz "${JANSSON_DIR}.tar.gz"

function build_for_arch() {
  pushd . > /dev/null
  cd "${JANSSON_DIR}"
  ARCH=$1
  HOST=$2
  SYSROOT=$3
  PREFIX=$4
  export CFLAGS="-arch ${ARCH} -Os -isysroot ${SYSROOT} -miphoneos-version-min=${IOS_MIN_SDK_VERSION} -fembed-bitcode"
  export LDFLAGS="-arch ${ARCH} -isysroot ${SYSROOT}"
  make clean
  ./configure --host="${HOST}" && make
  mkdir -p ${PREFIX}/lib/
  cp -p src/.libs/libjansson.a ${PREFIX}/lib/libjansson.a
  popd > /dev/null
}

TMP_DIR=/tmp/build_libjansson_$$

build_for_arch i386 i386-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk ${TMP_DIR}/i386 || exit 1
build_for_arch x86_64 x86_64-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk ${TMP_DIR}/x86_64 || exit 2
build_for_arch armv7 armv7-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk ${TMP_DIR}/armv7 || exit 3
build_for_arch arm64 arm-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk ${TMP_DIR}/arm64 || exit 4

mkdir -p ${TMP_DIR}/lib/

lipo \
  -arch i386 ${TMP_DIR}/i386/lib/libjansson.a \
  -arch x86_64 ${TMP_DIR}/x86_64/lib/libjansson.a \
  -arch armv7 ${TMP_DIR}/armv7/lib/libjansson.a \
  -arch arm64 ${TMP_DIR}/arm64/lib/libjansson.a \
  -output ${TMP_DIR}/lib/libjansson.a -create

mkdir -p ${DIST_DIR}/lib
mkdir -p ${DIST_DIR}/include
cp "${TMP_DIR}/lib/libjansson.a" "${DIST_DIR}/lib"

echo "Copying headers"
cp "${JANSSON_DIR}"/src/*.h "${DIST_DIR}"/include

echo "Cleaning up"
rm -rf ${TMP_DIR}
rm -rf ${JANSSON_DIR}
