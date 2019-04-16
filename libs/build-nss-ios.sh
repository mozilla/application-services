#!/usr/bin/env bash

# This script cross-compiles the NSS library for iOS.

set -euvx

if [ "${#}" -ne 4 ]
then
    echo "Usage:"
    echo "./build-nss-ios.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <IOS_MIN_SDK_VERSION>"
    exit 1
fi

NSS_SRC_DIR=${1}
DIST_DIR=${2}
ARCH=${3}
IOS_MIN_SDK_VERSION=${4}

if [ -d "${DIST_DIR}" ]; then
  echo "${DIST_DIR}"" folder already exists. Skipping build."
  exit 0
fi

if [[ "${ARCH}" == "i386" || "${ARCH}" == "x86_64" ]]; then
  OS_COMPILER="iPhoneSimulator"
  TARGET="x86_64-apple-darwin"
elif [[ "${ARCH}" == "armv7" || "${ARCH}" == "arm64" ]]; then
  OS_COMPILER="iPhoneOS"
  TARGET="aarch64-apple-darwin"
else
  echo "Unsupported architecture"
  exit 1
fi

DEVELOPER=$(xcode-select -print-path)
CROSS_TOP="${DEVELOPER}/Platforms/${OS_COMPILER}.platform/Developer"
CROSS_SDK="${OS_COMPILER}.sdk"
TOOLCHAIN_BIN="${DEVELOPER}/Toolchains/XcodeDefault.xctoolchain/usr/bin"
ISYSROOT="${CROSS_TOP}/SDKs/${CROSS_SDK}"
CC="${TOOLCHAIN_BIN}/clang -arch ${ARCH} -isysroot ${ISYSROOT} -lc++ -mios-version-min=${IOS_MIN_SDK_VERSION}"
CPU_ARCH="arm" # Static on purpose as NSS's Makefiles don't try to do anything funny when CPU_ARCH == "arm".

# Build NSPR
NSPR_BUILD_DIR=$(mktemp -d)
pushd "${NSPR_BUILD_DIR}"
"${NSS_SRC_DIR}"/nspr/configure \
  STRIP="${TOOLCHAIN_BIN}/strip" \
  RANLIB="${TOOLCHAIN_BIN}/ranlib" \
  AR="${TOOLCHAIN_BIN}/ar" \
  AS="${TOOLCHAIN_BIN}/as" \
  LD="${TOOLCHAIN_BIN}/ld -arch arm64" \
  CC="${CC}" \
  CCC="${CC}" \
  --target aarch64-apple-darwin \
  --enable-64bit \
  --disable-debug \
  --enable-optimize
make
popd

# Build NSS
BUILD_DIR=$(mktemp -d)
make \
  CROSS_COMPILE=1 \
  STRIP="${TOOLCHAIN_BIN}/strip" \
  RANLIB="${TOOLCHAIN_BIN}/ranlib" \
  AR="${TOOLCHAIN_BIN}/ar cr"' $@' \
  AS="${TOOLCHAIN_BIN}/as -arch ${ARCH} -isysroot ${ISYSROOT}" \
  LINK="${TOOLCHAIN_BIN}/ld -arch ${ARCH}" \
  CC="${CC}" \
  CCC="${CC}" \
  OS_ARCH=Darwin \
  OS_TEST="${CPU_ARCH}" \
  CPU_ARCH="${CPU_ARCH}" \
  USE_64=1 \
  BUILD_OPT=1 \
  NSS_DISABLE_CHACHAPOLY=1 \
  NSS_DISABLE_DBM=1 \
  BUILD_TREE="${BUILD_DIR}" \
  SOURCE_PREFIX="${BUILD_DIR}/dist" \
  SOURCE_MD_DIR="${BUILD_DIR}/dist" \
  DIST="${BUILD_DIR}/dist" \
  SOURCE_MDHEADERS_DIR="${NSPR_BUILD_DIR}/dist/include/nspr" \
  NSPR_INCLUDE_DIR="${NSPR_BUILD_DIR}/dist/include/nspr" \
  NSPR_LIB_DIR="${NSPR_BUILD_DIR}/dist/lib" \
  NSINSTALL="${NSPR_BUILD_DIR}/config/nsinstall" \
  -C "${NSS_SRC_DIR}/nss"

mkdir -p "${DIST_DIR}/include/nss"
mkdir -p "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libfreebl3.dylib "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libnss3.dylib "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libnssckbi.dylib "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libnssutil3.dylib "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libsmime3.dylib "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libsoftokn3.dylib "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libssl3.dylib "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libplc4.dylib "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libplds4.dylib "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libnspr4.dylib "${DIST_DIR}/lib"

cp -p -L "${BUILD_DIR}/dist"/public/nss/* "${DIST_DIR}/include/nss"
cp -p -L -R "${NSPR_BUILD_DIR}/dist"/include/nspr/* "${DIST_DIR}/include/nss"
