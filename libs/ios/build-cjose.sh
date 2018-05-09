#!/bin/bash

# This script downloads and builds the iOS cjose library with Bitcode enabled.

set -e

DIST_DIR=$(pwd)/cjose
CJOSE_VERSION="0.6.1"
CJOSE_DIR="cjose-${CJOSE_VERSION}"
IOS_MIN_SDK_VERSION="9.0"
OPENSSLDIR=$(pwd)/openssl
JANSSONDIR=$(pwd)/jansson

rm -rf "${CJOSE_DIR}"

if [ ! -e "${CJOSE_DIR}.tar.gz" ]; then
  echo "Downloading ${CJOSE_DIR}.tar.gz"
  curl -L "https://github.com/cisco/cjose/archive/${CJOSE_VERSION}.tar.gz" -o "${CJOSE_DIR}.tar.gz"
else
  echo "Using ${CJOSE_DIR}.tar.gz"
fi

echo "Unpacking ${CJOSE_DIR}"
tar xfz "${CJOSE_DIR}.tar.gz"

function build_for_arch() {
  pushd . > /dev/null
  cd "${CJOSE_DIR}"
  ARCH=$1
  HOST=$2
  SYSROOT=$3
  PREFIX=$4
  export CFLAGS="-arch ${ARCH} -Os -isysroot ${SYSROOT} -miphoneos-version-min=${IOS_MIN_SDK_VERSION} -fembed-bitcode"
  export LDFLAGS="-arch ${ARCH} -isysroot ${SYSROOT}"
  make clean
  ./configure --host="${HOST}" --with-openssl="${OPENSSLDIR}" --with-jansson="${JANSSONDIR}" && make
  mkdir -p ${PREFIX}/lib/
  cp -p src/.libs/libcjose.a ${PREFIX}/lib/libcjose.a
  popd > /dev/null
}

TMP_DIR=/tmp/build_libcjose_$$

build_for_arch i386 i386-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk ${TMP_DIR}/i386 || exit 1
build_for_arch x86_64 x86_64-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk ${TMP_DIR}/x86_64 || exit 2
build_for_arch armv7 armv7-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk ${TMP_DIR}/armv7 || exit 3
build_for_arch arm64 arm-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk ${TMP_DIR}/arm64 || exit 4

mkdir -p ${TMP_DIR}/lib/

lipo \
  -arch i386 ${TMP_DIR}/i386/lib/libcjose.a \
	-arch x86_64 ${TMP_DIR}/x86_64/lib/libcjose.a \
	-arch armv7 ${TMP_DIR}/armv7/lib/libcjose.a \
	-arch arm64 ${TMP_DIR}/arm64/lib/libcjose.a \
	-output ${TMP_DIR}/lib/libcjose.a -create

mkdir -p ${DIST_DIR}/lib
mkdir -p ${DIST_DIR}/include
cp "${TMP_DIR}/lib/libcjose.a" "${DIST_DIR}/lib"

echo "Copying headers"
cp "${CJOSE_DIR}"/include/cjose/*.h "${DIST_DIR}"/include

echo "Cleaning up"
rm -rf ${TMP_DIR}
rm -rf ${CJOSE_DIR}
