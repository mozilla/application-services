#!/usr/bin/env bash

# This script builds the NSS3 library (with NSPR) for Desktop.

set -euvx

if [[ "${#}" -lt 1 ]] || [[ "${#}" -gt 2 ]]
then
  echo "Usage:"
  echo "./build-nss-desktop.sh <ABSOLUTE_SRC_DIR> [CROSS_COMPILE_TARGET]"
  exit 1
fi

NSS_SRC_DIR=${1}
# Whether to cross compile from Linux to a different target.  Really
# only intended for automation.
CROSS_COMPILE_TARGET=${2-}

if [[ -n "${CROSS_COMPILE_TARGET}" ]] && [[ "$(uname -s)" != "Linux" ]]; then
  echo "Can only cross compile from 'Linux'; 'uname -s' is $(uname -s)"
  exit 1
fi

if [[ "${CROSS_COMPILE_TARGET}" =~ "darwin" ]]; then
  DIST_DIR=$(abspath "desktop/darwin/nss")
elif [[ -n "${CROSS_COMPILE_TARGET}" ]]; then
  echo "Cannot build NSS for unrecognized target OS ${CROSS_COMPILE_TARGET}"
  exit 1
elif [[ "$(uname -s)" == "Darwin" ]]; then
  if [[ "$(uname -m)" == "arm64" ]]; then
    DIST_DIR=$(abspath "desktop/darwin-aarch64/nss")
  else
    DIST_DIR=$(abspath "desktop/darwin-x86-64/nss")
  fi
elif [[ "$(uname -s)" == "Linux" ]]; then
  # This is a JNA weirdness: "x86-64" rather than "x86_64".
  DIST_DIR=$(abspath "desktop/linux-x86-64/nss")
else
   echo "Cannot build NSS on unrecognized host OS $(uname -s)"
   exit 1
fi

if [[ -d "${DIST_DIR}" ]]; then
  echo "${DIST_DIR} folder already exists. Skipping build."
  exit 0
fi

CROSS_COMPILE_ARGS=()
CHECK_NM="${NM:-nm}"
if [[ "${CROSS_COMPILE_TARGET}" =~ "darwin" ]]; then
  CROSS_COMPILE_ARGS=(--target-os=macos --macos-sdk="${MACOS_SDK:?set MACOS_SDK to an unpacked MacOSX.sdk}" -t x64)
  CHECK_NM=llvm-nm
fi
"${NSS_SRC_DIR}"/nss/build.sh \
  -v \
  --opt \
  --static \
  --disable-tests \
  --python=python3 \
  -Ddisable_dbm=1 \
  -Dsign_libs=0 \
  -Ddisable_libpkix=1 \
  ${CROSS_COMPILE_ARGS[@]+"${CROSS_COMPILE_ARGS[@]}"}
NSS_DIST_DIR="${NSS_SRC_DIR}/dist"

mkdir -p "${DIST_DIR}/include/nss"
NSS_DIST_OBJ_DIR="${NSS_DIST_DIR}/Release"
./copy-nss-libs.sh "${NSS_DIST_OBJ_DIR}/lib" "${DIST_DIR}/lib"

cp -p -L -R "${NSS_DIST_DIR}/public/nss/"* "${DIST_DIR}/include/nss"
cp -p -L -R "${NSS_DIST_OBJ_DIR}/include/nspr/"* "${DIST_DIR}/include/nss"

NM="${CHECK_NM}" ./check-nss-libs.sh "${NSS_DIST_OBJ_DIR}/lib" "${DIST_DIR}/lib"

rm -rf "${NSS_DIST_DIR}"
