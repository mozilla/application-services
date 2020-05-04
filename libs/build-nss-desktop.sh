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
  # Generated from nss-try@11e799981c28df3b4c36be1b5aabcca6f91ce798.
  curl -sfSL --retry 5 --retry-delay 10 -O "https://fxa-dev-bucket.s3-us-west-2.amazonaws.com/nss/nss_nspr_static_3.52_darwin.bz2"
  SHA256="c6ae59b3cd0dd8bd1e28c3dd7b26f720d220599e2ce7802cba81b884989e9a89"
  echo "${SHA256}  nss_nspr_static_3.52_darwin.bz2" | shasum -a 256 -c - || exit 2
  tar xvjf nss_nspr_static_3.52_darwin.bz2 && rm -rf nss_nspr_static_3.52_darwin.bz2
  NSS_DIST_DIR=$(abspath "dist")
elif [[ "${CROSS_COMPILE_TARGET}" =~ "win32-x86-64" ]]; then
  # Generated from nss-try@11e799981c28df3b4c36be1b5aabcca6f91ce798.
  curl -sfSL --retry 5 --retry-delay 10 -O "https://fxa-dev-bucket.s3-us-west-2.amazonaws.com/nss/nss_nspr_static_3.52_mingw.7z"
  SHA256="07fe3d0b0bc1b2cf51552fe0a7c8e6103b0a6b1baef5d4412dbfdcd0e1b834de"
  echo "${SHA256}  nss_nspr_static_3.52_mingw.7z" | shasum -a 256 -c - || exit 2
  7z x nss_nspr_static_3.52_mingw.7z -aoa && rm -rf nss_nspr_static_3.52_mingw.7z
  NSS_DIST_DIR=$(abspath "dist")
  # NSPR outputs .a files when cross-compiling.
  mv "${NSS_DIST_DIR}/Release/lib/libplc4.a" "${NSS_DIST_DIR}/Release/lib/libplc4.lib"
  mv "${NSS_DIST_DIR}/Release/lib/libplds4.a" "${NSS_DIST_DIR}/Release/lib/libplds4.lib"
  mv "${NSS_DIST_DIR}/Release/lib/libnspr4.a" "${NSS_DIST_DIR}/Release/lib/libnspr4.lib"
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

if [[ "${CROSS_COMPILE_TARGET}" =~ "win32-x86-64" ]]; then
  EXT="lib"
  PREFIX=""
elif [[ "$(uname -s)" == "Darwin" ]] || [[ "$(uname -s)" == "Linux" ]]; then
  EXT="a"
  PREFIX="lib"
fi

mkdir -p "${DIST_DIR}/include/nss"
mkdir -p "${DIST_DIR}/lib"
NSS_DIST_OBJ_DIR="${NSS_DIST_DIR}/Release"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}certdb.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}certhi.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}cryptohi.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}freebl_static.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}nss_static.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}nssb.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}nssdev.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}nsspki.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}nssutil.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}pk11wrap_static.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}pkcs12.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}pkcs7.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}smime.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}softokn_static.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}ssl.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}hw-acc-crypto-avx.${EXT}" "${DIST_DIR}/lib"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}hw-acc-crypto-avx2.${EXT}" "${DIST_DIR}/lib"

# HW specific.
# https://searchfox.org/mozilla-central/rev/1eb05019f47069172ba81a6c108a584a409a24ea/security/nss/lib/freebl/freebl.gyp#159-163
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}gcm-aes-x86_c_lib.${EXT}" "${DIST_DIR}/lib"
# https://searchfox.org/mozilla-central/rev/1eb05019f47069172ba81a6c108a584a409a24ea/security/nss/lib/freebl/freebl.gyp#224-233
if [[ "${TARGET_OS}" == "windows" ]] || [[ "${TARGET_OS}" == "linux" ]]; then
  cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}intel-gcm-wrap_c_lib.${EXT}" "${DIST_DIR}/lib"
  # https://searchfox.org/mozilla-central/rev/1eb05019f47069172ba81a6c108a584a409a24ea/security/nss/lib/freebl/freebl.gyp#43-47
  if [[ "${TARGET_OS}" == "linux" ]]; then
    cp -p -L "${NSS_DIST_OBJ_DIR}/lib/${PREFIX}intel-gcm-s_lib.${EXT}" "${DIST_DIR}/lib"
  fi
fi

# For some reason the NSPR libs always have the "lib" prefix even on Windows.
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libplc4.${EXT}" "${DIST_DIR}/lib/${PREFIX}plc4.${EXT}"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libplds4.${EXT}" "${DIST_DIR}/lib/${PREFIX}plds4.${EXT}"
cp -p -L "${NSS_DIST_OBJ_DIR}/lib/libnspr4.${EXT}" "${DIST_DIR}/lib/${PREFIX}nspr4.${EXT}"

cp -p -L -R "${NSS_DIST_DIR}/public/nss/"* "${DIST_DIR}/include/nss"
cp -p -L -R "${NSS_DIST_OBJ_DIR}/include/nspr/"* "${DIST_DIR}/include/nss"

rm -rf "${NSS_DIST_DIR}"
