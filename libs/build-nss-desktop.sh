#!/usr/bin/env bash

# This script builds the NSS3 library (with NSPR) for Desktop.

set -euvx

if [ "${#}" -lt 1 -o "${#}" -gt 2 ]
then
  echo "Usage:"
  echo "./build-nss-desktop.sh <NSS_SRC_PATH> [CROSS_COMPILE_TARGET]"
  exit 1
fi

NSS_SRC_PATH=${1}
# Whether to cross compile from Linux to a different target.  Really
# only intended for automation.
CROSS_COMPILE_TARGET=${2-}

if [ -n "${CROSS_COMPILE_TARGET}" -a $(uname -s) != "Linux" ]; then
  echo "Can only cross compile from 'Linux'; 'uname -s' is $(uname -s)"
  exit 1
fi

if [[ "${CROSS_COMPILE_TARGET}" =~ "win32-x86-64" ]]; then
  NSS_DIR=$(abspath "desktop/win32-x86-64/nss")
elif [[ "${CROSS_COMPILE_TARGET}" =~ "darwin" ]]; then
  NSS_DIR=$(abspath "desktop/darwin/nss")
elif [ -n "${CROSS_COMPILE_TARGET}" ]; then
  echo "Cannot build NSS for unrecognized target OS ${CROSS_COMPILE_TARGET}"
  exit 1
elif [ $(uname -s) == "Darwin" ]; then
  NSS_DIR=$(abspath "desktop/darwin/nss")
elif [ $(uname -s) == "Linux" ]; then
  # This is a JNA weirdness: "x86-64" rather than "x86_64".
  NSS_DIR=$(abspath "desktop/linux-x86-64/nss")
else
   echo "Cannot build NSS on unrecognized host OS $(uname -s)"
   exit 1
fi

if [ -d "${NSS_DIR}" ]; then
  echo "${NSS_DIR} folder already exists. Skipping build."
  exit 0
fi

NSPR_BUILD_DIR=$(mktemp -d)
BUILD_DIR=$(mktemp -d)

EXTRA_MAKE_ARGS=()
# Build NSPR.
pushd "${NSPR_BUILD_DIR}"
if [[ "${CROSS_COMPILE_TARGET}" =~ "darwin" ]]; then
  # TODO cross compile ourselves, I lost my sanity over this and gave up.
  curl -L -O "https://s3-us-west-2.amazonaws.com/fxa-dev-bucket/nss/nss-dist.tar.bz2"
  SHA256="e744a4e0ea7daad75b28eef63d6ced0acd8a993a850998018916e0cad82dc382"
  echo "${SHA256}  nss-dist.tar.bz2" | shasum -a 256 -c - || exit 2
  tar xvjf nss-dist.tar.bz2
  mkdir -p "${NSS_DIR}/include/nss"
  mkdir -p "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libnss3.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libnssutil3.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libfreebl3.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libnssckbi.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libsmime3.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libsoftokn3.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libssl3.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libplc4.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libplds4.dylib "${NSS_DIR}/lib"
  cp -p -L dist/Debug/lib/libnspr4.dylib "${NSS_DIR}/lib"
  cp -p -L dist/public/nss/*.h "${NSS_DIR}/include/nss"
  cp -p -L -R dist/Debug/include/nspr/* "${NSS_DIR}/include/nss"
  rm -rf dist && rm -f nss-dist.tar.bz2
  exit 0

elif [[ "${CROSS_COMPILE_TARGET}" =~ "win32-x86-64" ]]; then
  # Build NSPR.
  "${NSS_SRC_PATH}"/nspr/configure \
    --target x86_64-w64-mingw32 \
    --enable-64bit \
    --disable-debug \
    --enable-optimize
  EXTRA_MAKE_ARGS+=('OS_ARCH=WINNT')
  EXTRA_MAKE_ARGS+=('OS_TARGET=WIN95')
  EXTRA_MAKE_ARGS+=('NS_USE_GCC=1')
  EXTRA_MAKE_ARGS+=('CC=x86_64-w64-mingw32-gcc')
  EXTRA_MAKE_ARGS+=('CCC=x86_64-w64-mingw32-gcc')
  EXTRA_MAKE_ARGS+=('RC=x86_64-w64-mingw32-windres -O coff --use-temp-file')
elif [ "$(uname -s)" == "Darwin" -o "$(uname -s)" == "Linux" ]; then
  "${NSS_SRC_PATH}"/nspr/configure \
    --enable-64bit \
    --disable-debug \
    --enable-optimize
fi
make
popd

# Build NSS.
make \
  ${EXTRA_MAKE_ARGS[@]+"${EXTRA_MAKE_ARGS[@]}"} \
  USE_64=1 \
  BUILD_OPT=1 \
  NSS_DISABLE_CHACHAPOLY=1 \
  NSS_DISABLE_DBM=1 \
  SOURCE_MDHEADERS_DIR="${NSPR_BUILD_DIR}/dist/include/nspr" \
  NSPR_INCLUDE_DIR="${NSPR_BUILD_DIR}/dist/include/nspr" \
  NSPR_LIB_DIR="${NSPR_BUILD_DIR}/dist/lib" \
  NSINSTALL="${NSPR_BUILD_DIR}/config/nsinstall" \
  BUILD_TREE="${BUILD_DIR}" \
  SOURCE_PREFIX="${BUILD_DIR}/dist" \
  SOURCE_MD_DIR="${BUILD_DIR}/dist" \
  DIST="${BUILD_DIR}/dist" \
  -C ${NSS_SRC_PATH}/nss

mkdir -p "${NSS_DIR}/include/nss"
mkdir -p "${NSS_DIR}/lib"

if [[ "${CROSS_COMPILE_TARGET}" =~ "win32-x86-64" ]]; then
  EXT="dll"
  PREFIX=""
elif [ "$(uname -s)" == "Darwin" -o "$(uname -s)" == "Linux" ]; then
  [[ "$(uname -s)" == "Darwin" ]] && EXT="dylib" || EXT="so"
  PREFIX="lib"
fi

cp -p -L "${BUILD_DIR}/dist"/lib/"${PREFIX}"freebl3."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/"${PREFIX}"nss3."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/"${PREFIX}"nssckbi."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/"${PREFIX}"nssutil3."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/"${PREFIX}"smime3."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/"${PREFIX}"softokn3."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${BUILD_DIR}/dist"/lib/"${PREFIX}"ssl3."${EXT}" "${NSS_DIR}/lib"
# For some reason the NSPR libs always have the "lib" prefix even on Windows.
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libplc4."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libplds4."${EXT}" "${NSS_DIR}/lib"
cp -p -L "${NSPR_BUILD_DIR}/dist"/lib/libnspr4."${EXT}" "${NSS_DIR}/lib"

cp -p -L "${BUILD_DIR}/dist"/public/nss/* "${NSS_DIR}/include/nss"
cp -p -L -R "${NSPR_BUILD_DIR}/dist"/include/nspr/* "${NSS_DIR}/include/nss"
