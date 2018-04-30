#!/bin/bash

export DEVROOT=/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain
DFT_DIST_DIR=${HOME}/libjansson-ios-dist
DIST_DIR=${DIST_DIR:-$DFT_DIST_DIR}

function build_for_arch() {
  ARCH=$1
  HOST=$2
  SYSROOT=$3
  PREFIX=$4
  IPHONEOS_DEPLOYMENT_TARGET="9.0"
  export PATH="${DEVROOT}/usr/bin/:${PATH}"
  export CFLAGS="-arch ${ARCH} -pipe -Os -gdwarf-2 -isysroot ${SYSROOT} -miphoneos-version-min=${IPHONEOS_DEPLOYMENT_TARGET} -fembed-bitcode"
  export LDFLAGS="-arch ${ARCH} -isysroot ${SYSROOT}"
  make clean
  ./configure --host="${HOST}" && make
  mkdir -p ${PREFIX}/lib/
  cp -p src/.libs/libjansson.a ${PREFIX}/lib/libjansson.a
}

TMP_DIR=/tmp/build_libjansson_$$

#build_for_arch i386 i386-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk ${TMP_DIR}/i386 || exit 1
build_for_arch x86_64 x86_64-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk ${TMP_DIR}/x86_64 || exit 2
build_for_arch armv7s armv7s-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk ${TMP_DIR}/armv7s || exit 4
#build_for_arch armv7 armv7-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk ${TMP_DIR}/armv7 || exit 5
build_for_arch arm64 arm-apple-darwin /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk ${TMP_DIR}/arm64 || exit 3

mkdir -p ${TMP_DIR}/lib/

${DEVROOT}/usr/bin/lipo \
	-arch x86_64 ${TMP_DIR}/x86_64/lib/libjansson.a \
	-arch armv7s ${TMP_DIR}/armv7s/lib/libjansson.a \
	-arch arm64 ${TMP_DIR}/arm64/lib/libjansson.a \
	-output ${TMP_DIR}/lib/libjansson.a -create


mkdir -p ${DIST_DIR}/include
cp -p src/*.h ${DIST_DIR}/include
cp -r ${TMP_DIR}/lib ${DIST_DIR}
