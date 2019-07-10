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
  echo "${DIST_DIR} folder already exists. Skipping build."
  exit 0
fi

PLATFORM_PATH="${ANDROID_NDK_ROOT}/platforms/android-${ANDROID_NDK_API_VERSION}/arch-${ARCH}"
if [ "${TOOLCHAIN}" == "x86_64-linux-android" ]
then
  GYP_ARCH="x64"
  LDFLAGS="-L${PLATFORM_PATH}/usr/lib64"
  NSPR_64="--enable-64bit"
elif [ "${TOOLCHAIN}" == "i686-linux-android" ]
then
  GYP_ARCH="ia32"
elif [ "${TOOLCHAIN}" == "aarch64-linux-android" ]
then
  GYP_ARCH="arm64"
  NSPR_64="--enable-64bit"
elif [ "${TOOLCHAIN}" == "arm-linux-androideabi" ]
then
  GYP_ARCH="arm"
else
  echo "Unknown toolchain"
  exit 1
fi
LDFLAGS="${LDFLAGS:-}"
NSPR_64="${NSPR_64:-""}"

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
export AR="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-ar"
export CC="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-clang"
export CXX="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-clang++"
export LD="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-ld"
export NM="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-nm"
export RANLIB="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-ranlib"
export READELF="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}-readelf"

BUILD_DIR=$(mktemp -d)
rm -rf "${NSS_SRC_DIR}/nss/out"
gyp -f ninja-android "${NSS_SRC_DIR}/nss/nss.gyp" \
  --depth "${NSS_SRC_DIR}/nss/" \
  --generator-output=. \
  -DOS=android \
  -Dnspr_lib_dir="${NSPR_BUILD_DIR}/dist/lib" \
  -Dnspr_include_dir="${NSPR_BUILD_DIR}/dist/include/nspr" \
  -Dnss_dist_dir="${BUILD_DIR}" \
  -Dnss_dist_obj_dir="${BUILD_DIR}" \
  -Dhost_arch="${GYP_ARCH}" \
  -Dtarget_arch="${GYP_ARCH}" \
  -Dstatic_libs=1 \
  -Ddisable_dbm=1 \
  -Dsign_libs=0 \
  -Denable_sslkeylogfile=0 \
  -Ddisable_tests=1 \
  -Ddisable_libpkix=1

GENERATED_DIR="${NSS_SRC_DIR}/nss/out/Release"
ninja -C "${GENERATED_DIR}"

mkdir -p "${DIST_DIR}/include/nss"
mkdir -p "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libcertdb.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libcerthi.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libcryptohi.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libfreebl_static.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libnss_static.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libnssb.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libnssdev.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libnsspki.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libnssutil.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libpk11wrap_static.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libpkcs12.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libpkcs7.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libsmime.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libsoftokn_static.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libssl.a" "${DIST_DIR}/lib"
cp -p -L "${BUILD_DIR}/lib/libhw-acc-crypto.a" "${DIST_DIR}/lib"
# HW specific.
if [[ "${TOOLCHAIN}" == "i686-linux-android" ]] || [[ "${TOOLCHAIN}" == "x86_64-linux-android" ]]; then
  cp -p -L "${BUILD_DIR}/lib/libgcm-aes-x86_c_lib.a" "${DIST_DIR}/lib"
fi
cp -p -L "${NSPR_BUILD_DIR}/dist/lib/libplc4.a" "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist/lib/libplds4.a" "${DIST_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist/lib/libnspr4.a" "${DIST_DIR}/lib"

cp -p -L -R "${BUILD_DIR}/public/nss/"* "${DIST_DIR}/include/nss"
cp -p -L -R "${NSPR_BUILD_DIR}/dist/include/nspr/"* "${DIST_DIR}/include/nss"
