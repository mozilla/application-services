#!/usr/bin/env bash

# Common script to copy NSS and NSPR libraries to a distribution directory
# Usage: copy-nss-libs.sh <TARGET_OS> <ARCH> <DIST_DIR> <NSS_LIB_DIR> <NSPR_LIB_DIR> <NSS_INCLUDE_DIR> <NSPR_INCLUDE_DIR>

set -euvx

if [[ "${#}" -ne 7 ]]
then
  echo "Usage:"
  echo "./copy-nss-libs.sh <TARGET_OS> <ARCH> <DIST_DIR> <NSS_LIB_DIR> <NSPR_LIB_DIR> <NSS_INCLUDE_DIR> <NSPR_INCLUDE_DIR>"
  exit 1
fi

TARGET_OS=${1}
ARCH=${2}
DIST_DIR=${3}
NSS_LIB_DIR=${4}
NSPR_LIB_DIR=${5}
NSS_INCLUDE_DIR=${6}
NSPR_INCLUDE_DIR=${7}

mkdir -p "${DIST_DIR}/include/nss"
mkdir -p "${DIST_DIR}/lib"

# NSPR libraries
cp -p -L "${NSPR_LIB_DIR}/libplc4.a" "${DIST_DIR}/lib"
cp -p -L "${NSPR_LIB_DIR}/libplds4.a" "${DIST_DIR}/lib"
cp -p -L "${NSPR_LIB_DIR}/libnspr4.a" "${DIST_DIR}/lib"

# NSS libraries
cp -p -L "${NSS_LIB_DIR}/libcertdb.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libcerthi.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libcryptohi.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libfreebl_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libnss_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libmozpkix.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libnssb.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libnssdev.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libnsspki.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libnssutil.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libpk11wrap_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libpkcs12.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libpkcs7.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libsmime.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libsoftokn_static.a" "${DIST_DIR}/lib"
cp -p -L "${NSS_LIB_DIR}/libssl.a" "${DIST_DIR}/lib"

# Architecture or Platform specific libraries
# https://searchfox.org/firefox-main/rev/7d8644b9d4470a675bf670c2dc7664cc01f14ece/security/nss/lib/freebl/freebl.gyp
if [[ "${ARCH}" == "x86_64" ]]; then
  cp -p -L "${NSS_LIB_DIR}/libhw-acc-crypto-avx.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_LIB_DIR}/libhw-acc-crypto-avx2.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_LIB_DIR}/libgcm-aes-x86_c_lib.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_LIB_DIR}/libsha-x86_c_lib.a" "${DIST_DIR}/lib"
fi
if [[ "${ARCH}" == "aarch64" ]] || [[ "${ARCH}" == "arm64" ]]; then
  cp -p -L "${NSS_LIB_DIR}/libarmv8_c_lib.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_LIB_DIR}/libgcm-aes-aarch64_c_lib.a" "${DIST_DIR}/lib"
fi
if [[ "${TARGET_OS}" == "linux" ]]; then
  cp -p -L "${NSS_LIB_DIR}/libintel-gcm-wrap_c_lib.a" "${DIST_DIR}/lib"
  cp -p -L "${NSS_LIB_DIR}/libintel-gcm-s_lib.a" "${DIST_DIR}/lib"
fi

# Copy headers
cp -p -L -R "${NSS_INCLUDE_DIR}/"* "${DIST_DIR}/include/nss"
cp -p -L -R "${NSPR_INCLUDE_DIR}/"* "${DIST_DIR}/include/nss"
