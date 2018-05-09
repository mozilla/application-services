#!/bin/bash

# This script downloads and builds the iOS, tvOS and Mac openSSL libraries with Bitcode enabled

# Credits:
# https://github.com/st3fan/ios-openssl
# https://github.com/x2on/OpenSSL-for-iPhone/blob/master/build-libssl.sh
# https://gist.github.com/foozmeat/5154962
# Peter Steinberger, PSPDFKit GmbH, @steipete.
# Felix Schwarz, IOSPIRIT GmbH, @felix_schwarz.

set -e

IOS_MIN_SDK_VERSION="9.0"

OPENSSL_VERSION="openssl-1.0.2o"
DEVELOPER=`xcode-select -print-path`
DIST_DIR=$(pwd)/openssl

buildIOS()
{
  ARCH=$1

  pushd . > /dev/null
  cd "${OPENSSL_VERSION}"

  if [[ "${ARCH}" == "i386" || "${ARCH}" == "x86_64" ]]; then
    PLATFORM="iPhoneSimulator"
  else
    PLATFORM="iPhoneOS"
    sed -ie "s!static volatile sig_atomic_t intr_signal;!static volatile intr_signal;!" "crypto/ui/ui_openssl.c"
  fi

  export $PLATFORM
  export CROSS_TOP="${DEVELOPER}/Platforms/${PLATFORM}.platform/Developer"
  export CROSS_SDK="${PLATFORM}.sdk"
  export BUILD_TOOLS="${DEVELOPER}"
  export CC="${BUILD_TOOLS}/usr/bin/gcc -fembed-bitcode -arch ${ARCH}"

  echo "Building ${OPENSSL_VERSION} for ${PLATFORM} ${ARCH}"

  if [[ "${ARCH}" == "x86_64" ]]; then
    ./Configure no-asm darwin64-x86_64-cc --openssldir="/tmp/${OPENSSL_VERSION}-iOS-${ARCH}" &> "/tmp/${OPENSSL_VERSION}-iOS-${ARCH}.log"
  else
    ./Configure iphoneos-cross --openssldir="/tmp/${OPENSSL_VERSION}-iOS-${ARCH}" &> "/tmp/${OPENSSL_VERSION}-iOS-${ARCH}.log"
  fi
  # add -isysroot to CC=
  sed -ie "s!^CFLAG=!CFLAG=-isysroot ${CROSS_TOP}/SDKs/${CROSS_SDK} -miphoneos-version-min=${IOS_MIN_SDK_VERSION} !" "Makefile"

  make >> "/tmp/${OPENSSL_VERSION}-iOS-${ARCH}.log" 2>&1
  make install_sw >> "/tmp/${OPENSSL_VERSION}-iOS-${ARCH}.log" 2>&1
  make clean >> "/tmp/${OPENSSL_VERSION}-iOS-${ARCH}.log" 2>&1
  popd > /dev/null
}

echo "Cleaning up"

mkdir -p ${DIST_DIR}/lib
mkdir -p ${DIST_DIR}/include/openssl

rm -rf "/tmp/${OPENSSL_VERSION}-*"
rm -rf "/tmp/${OPENSSL_VERSION}-*.log"

rm -rf "${OPENSSL_VERSION}"

if [ ! -e ${OPENSSL_VERSION}.tar.gz ]; then
  echo "Downloading ${OPENSSL_VERSION}.tar.gz"
  curl -O https://www.openssl.org/source/${OPENSSL_VERSION}.tar.gz
else
  echo "Using ${OPENSSL_VERSION}.tar.gz"
fi

echo "Unpacking openssl"
tar xfz "${OPENSSL_VERSION}.tar.gz"

buildIOS "armv7"
buildIOS "arm64"
buildIOS "x86_64"
buildIOS "i386"

mkdir -p "/tmp/${OPENSSL_VERSION}-iOS-lipo/lib/"

echo "Building iOS libraries"
lipo \
  -arch i386 "/tmp/${OPENSSL_VERSION}-iOS-i386/lib/libcrypto.a" \
  -arch x86_64 "/tmp/${OPENSSL_VERSION}-iOS-x86_64/lib/libcrypto.a" \
  -arch armv7 "/tmp/${OPENSSL_VERSION}-iOS-armv7/lib/libcrypto.a" \
  -arch arm64 "/tmp/${OPENSSL_VERSION}-iOS-arm64/lib/libcrypto.a" \
  -output "/tmp/${OPENSSL_VERSION}-iOS-lipo/lib/libcrypto.a" -create

lipo \
  -arch i386 "/tmp/${OPENSSL_VERSION}-iOS-i386/lib/libssl.a" \
  -arch x86_64 "/tmp/${OPENSSL_VERSION}-iOS-x86_64/lib/libssl.a" \
  -arch armv7 "/tmp/${OPENSSL_VERSION}-iOS-armv7/lib/libssl.a" \
  -arch arm64 "/tmp/${OPENSSL_VERSION}-iOS-arm64/lib/libssl.a" \
  -output "/tmp/${OPENSSL_VERSION}-iOS-lipo/lib/libssl.a" -create

cp "/tmp/${OPENSSL_VERSION}-iOS-lipo/lib/libcrypto.a" "${DIST_DIR}/lib"
cp "/tmp/${OPENSSL_VERSION}-iOS-lipo/lib/libssl.a" "${DIST_DIR}/lib"

echo "Copying headers"
cp -L "${OPENSSL_VERSION}"/include/openssl/*.h "${DIST_DIR}/include/openssl"

echo "Cleaning up"
rm -rf /tmp/${OPENSSL_VERSION}-*
rm -rf ${OPENSSL_VERSION}

echo "Done"
