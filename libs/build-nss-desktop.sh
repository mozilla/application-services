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
# We only need this in a couple of places so we'll default to "unknown"
# Othertimes, it'll match what CARGO_CFG_TARGET_ARCH is on the rust side
TARGET_ARCH="unknown"

if [[ -n "${CROSS_COMPILE_TARGET}" ]] && [[ "$(uname -s)" != "Linux" ]]; then
  echo "Can only cross compile from 'Linux'; 'uname -s' is $(uname -s)"
  exit 1
fi

if [[ "${CROSS_COMPILE_TARGET}" =~ "win32-x86-64" ]]; then
  DIST_DIR=$(abspath "desktop/win32-x86-64/nss")
  TARGET_OS="windows"
elif [[ "${CROSS_COMPILE_TARGET}" =~ "darwin" ]]; then
  DIST_DIR=$(abspath "desktop/darwin/nss")
  TARGET_OS="macos"
elif [[ -n "${CROSS_COMPILE_TARGET}" ]]; then
  echo "Cannot build NSS for unrecognized target OS ${CROSS_COMPILE_TARGET}"
  exit 1
elif [[ "$(uname -s)" == "Darwin" ]]; then
  DIST_DIR=$(abspath "desktop/darwin/nss")
  TARGET_OS="macos"
  # We need to set this variable for switching libs based on different macos archs (M1 vs Intel)
  if [[ "$(uname -m)" == "arm64" ]]; then
    TARGET_ARCH="aarch64"
  else
    TARGET_ARCH="x86_64"
  fi
elif [[ "$(uname -s)" == "Linux" ]]; then
  # This is a JNA weirdness: "x86-64" rather than "x86_64".
  DIST_DIR=$(abspath "desktop/linux-x86-64/nss")
  TARGET_OS="linux"
else
   echo "Cannot build NSS on unrecognized host OS $(uname -s)"
   exit 1
fi

if [[ -d "${DIST_DIR}" ]]; then
  echo "${DIST_DIR} folder already exists. Skipping build."
  exit 0
fi

# TODO We do not know how to cross compile these, so we cheat by downloading them and the how is pretty disgusting.
# https://github.com/mozilla/application-services/issues/962
if [[ "${CROSS_COMPILE_TARGET}" =~ "darwin" ]]; then
  # Generated from nss-try@111b54aaa644978464bec98848ecba6f69d3f42e.
  curl -sfSL --retry 5 --retry-delay 10 -O "https://fxa-dev-bucket.s3-us-west-2.amazonaws.com/a-s/nss_nspr_static_3.65_darwin.tar.bz2"
  SHA256="d309f8d9ad94545760773926544e6a7a3a31a2ad3a76c13e284429e4ed83b5cd"
  echo "${SHA256}  nss_nspr_static_3.65_darwin.tar.bz2" | shasum -a 256 -c - || exit 2
  tar xvjf nss_nspr_static_3.65_darwin.tar.bz2 && rm -rf nss_nspr_static_3.65_darwin.tar.bz2
  NSS_DIST_DIR=$(abspath "dist")
elif [[ "${CROSS_COMPILE_TARGET}" =~ "win32-x86-64" ]]; then
  # Generated from nss-try@111b54aaa644978464bec98848ecba6f69d3f42e.
  curl -sfSL --retry 5 --retry-delay 10 -O "https://fxa-dev-bucket.s3-us-west-2.amazonaws.com/a-s/nss_nspr_static_3.65_mingw.7z"
  SHA256="feb6901a774a2c6e6b0aa40704f903b422d6fdf315679fc502d1edc6ef07b126"
  echo "${SHA256}  nss_nspr_static_3.65_mingw.7z" | shasum -a 256 -c - || exit 2
  7z x nss_nspr_static_3.65_mingw.7z -aoa && rm -rf nss_nspr_static_3.65_mingw.7z
  NSS_DIST_DIR=$(abspath "dist")
elif [[ "$(uname -s)" == "Darwin" ]] || [[ "$(uname -s)" == "Linux" ]]; then
  "${NSS_SRC_DIR}"/nss/build.sh \
    -v \
    --opt \
    --static \
    --disable-tests \
    -Ddisable_dbm=1 \
    -Dsign_libs=0 \
    -Ddisable_libpkix=1
  NSS_DIST_DIR="${NSS_SRC_DIR}/dist"
fi

mkdir -p "${DIST_DIR}/include/nss"
mkdir -p "${DIST_DIR}/lib"
NSS_DIST_OBJ_DIR="${NSS_DIST_DIR}/Release"
# NSPR
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libplc4.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libplds4.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libnspr4.a" "${DIST_DIR}/lib"
# NSS
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libcertdb.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libcerthi.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libcryptohi.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libfreebl_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libnss_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libnssb.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libnssdev.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libnsspki.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libnssutil.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libpk11wrap_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libpkcs12.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libpkcs7.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libsmime.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libsoftokn_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libssl.a" "${DIST_DIR}/lib"

# Apple M1 need HW specific libs copied over to successfully build
if [[ "${TARGET_ARCH}" == "aarch64" ]]; then
  cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libarmv8_c_lib.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libgcm-aes-aarch64_c_lib.a" "${DIST_DIR}/lib"
else
  # HW specific.
  # https://searchfox.org/mozilla-central/rev/1eb05019f47069172ba81a6c108a584a409a24ea/security/nss/lib/freebl/freebl.gyp#159-163
  cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libhw-acc-crypto-avx.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libhw-acc-crypto-avx2.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libgcm-aes-x86_c_lib.a" "${DIST_DIR}/lib"
fi
# https://searchfox.org/mozilla-central/rev/1eb05019f47069172ba81a6c108a584a409a24ea/security/nss/lib/freebl/freebl.gyp#224-233
if [[ "${TARGET_OS}" == "windows" ]] || [[ "${TARGET_OS}" == "linux" ]]; then
  cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libintel-gcm-wrap_c_lib.a" "${DIST_DIR}/lib"
  # https://searchfox.org/mozilla-central/rev/1eb05019f47069172ba81a6c108a584a409a24ea/security/nss/lib/freebl/freebl.gyp#43-47
  if [[ "${TARGET_OS}" == "linux" ]]; then
    cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libintel-gcm-s_lib.a" "${DIST_DIR}/lib"
  fi
fi

cp -p -L -R "${NSS_DIST_DIR}/public/nss/"* "${DIST_DIR}/include/nss"
cp -p -L -R "${NSS_DIST_OBJ_DIR}/include/nspr/"* "${DIST_DIR}/include/nss"

rm -rf "${NSS_DIST_DIR}"
