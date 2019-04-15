#!/usr/bin/env bash

# This script cross-compiles the NSS library for Android.

set -euvx

if [ "${#}" -ne 6 ]
then
    echo "Usage:"
    echo "./build-nss-android.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <TOOLCHAIN_PATH> <TOOLCHAIN> <ANDROID_NDK_API_VERSION>"
    exit 1
fi

NSS_SRC_DIR=${1}
DIST_DIR=${2}
ARCH=${3}
TOOLCHAIN_PATH=${4}
TOOLCHAIN=${5}
ANDROID_NDK_API_VERSION=${6}

if [ -d "${DIST_DIR}" ]; then
  echo "${DIST_DIR}"" folder already exists. Skipping build."
  exit 0
fi

PLATFORM_PATH="${ANDROID_NDK_ROOT}/platforms/android-${ANDROID_NDK_API_VERSION}/arch-${ARCH}"
USE_64=""
if [ "${TOOLCHAIN}" == "x86_64-linux-android" ]
then
  CONFIGURE_ARCH="android64-x86_64"
  CPU_ARCH="x86_64"
  LDFLAGS="-L${PLATFORM_PATH}/usr/lib64"
  USE_64=1
elif [ "${TOOLCHAIN}" == "i686-linux-android" ]
then
  CONFIGURE_ARCH="android-x86"
  CPU_ARCH="x86"
elif [ "${TOOLCHAIN}" == "aarch64-linux-android" ]
then
  CONFIGURE_ARCH="android-arm64"
  CPU_ARCH="arm"
  USE_64=1
elif [ "${TOOLCHAIN}" == "arm-linux-androideabi" ]
then
  CONFIGURE_ARCH="android-arm"
  CPU_ARCH="arm"
else
  echo "Unknown toolchain"
  exit 1
fi
NSPR_64=""
NSS_64=""
if [[ -n "${USE_64}" ]]; then
  NSPR_64="--enable-64bit"
  NSS_64="USE_64=1"
fi
LDFLAGS=${LDFLAGS:-}

# Build NSPR
NSPR_BUILD_DIR=$(mktemp -d)
pushd "${NSPR_BUILD_DIR}"
"${NSS_SRC_DIR}"/nspr/configure \
  LDFLAGS="${LDFLAGS}" \
  ${NSPR_64} \
  --target="${TOOLCHAIN}" \
  --with-android-ndk="${ANDROID_NDK_ROOT}" \
  --with-android-toolchain="${TOOLCHAIN_PATH}" \
  --with-android-platform="${PLATFORM_PATH}" \
   --disable-debug \
   --enable-optimize
make
popd

# Build NSS
BUILD_DIR=$(mktemp -d)
# The ANDROID_ vars are just set so the Makefile doesn't complain.
make \
  CROSS_COMPILE=1 \
  ANDROID_NDK=${ANDROID_NDK_ROOT} \
  ANDROID_TOOLCHAIN_VERSION=${ANDROID_NDK_API_VERSION} \
  CC="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-clang" \
  CCC="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-clang++" \
  RANLIB="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-ranlib" \
  OS_TARGET=Android \
  ${NSS_64} \
  LDFLAGS="${LDFLAGS}" \
  CPU_ARCH="${CPU_ARCH}" \
  ARCHFLAG="-D__ANDROID_API__=${ANDROID_NDK_API_VERSION}" \
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
cp -p -L "${BUILD_DIR}/dist"/lib/libfreebl3.so "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libnss3.so "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libnssckbi.so "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libnssutil3.so "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libsmime3.so "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libsoftokn3.so "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/libssl3.so "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libplc4.so "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libplds4.so "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libnspr4.so "${DIST_DIR}/lib"

cp -p -L "${BUILD_DIR}/dist"/public/nss/* "${DIST_DIR}/include/nss"
cp -p -L -R "${NSPR_BUILD_DIR}/dist"/include/nspr/* "${DIST_DIR}/include/nss"
