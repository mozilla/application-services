#!/usr/bin/env bash

set -euvx

IOS_MIN_SDK_VERSION="11.0"
# Our short-names for the architectures.
TARGET_ARCHS=("x86_64" "arm64")

if [ "${#}" -ne 3 ]
then
    echo "Usage:"
    echo "./build-all-ios.sh <OPENSSL_SRC_PATH> <SQLCIPHER_SRC_PATH> <NSS_SRC_PATH>"
    exit 1
fi

OPENSSL_SRC_PATH=${1}
SQLCIPHER_SRC_PATH=${2}
NSS_SRC_PATH=${3}

function universal_lib() {
  DIR_NAME="${1}"
  LIB_NAME="${2}"
  shift; shift
  UNIVERSAL_DIR="ios/universal/${DIR_NAME}"
  LIB_PATH="${UNIVERSAL_DIR}/lib/${LIB_NAME}"
  if [ ! -e "${LIB_PATH}" ]; then
    mkdir -p "${UNIVERSAL_DIR}/lib"
    CMD="lipo"
    for ARCH in "${@}"; do
      CMD="${CMD} -arch ${ARCH} ios/${ARCH}/${DIR_NAME}/lib/${LIB_NAME}"
    done
    CMD="${CMD} -output ${LIB_PATH} -create"
    ${CMD}
  fi
}

echo "# Building NSS"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[${i}]}
  DIST_DIR=$(abspath "ios/${ARCH}/nss")
  if [ -d "${DIST_DIR}" ]; then
    echo "${DIST_DIR} already exists. Skipping building nss."
  else
    ./build-nss-ios.sh "${NSS_SRC_PATH}" "${DIST_DIR}" "${ARCH}" "${IOS_MIN_SDK_VERSION}" || exit 1
  fi
done
universal_lib "nss" "libcertdb.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libfreebl_static.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libnssb.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libnssutil.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libpkcs7.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libsmime.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libcerthi.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libnspr4.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libnssdev.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libpk11wrap_static.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libplc4.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libsoftokn_static.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libcryptohi.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libnss_static.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libnsspki.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libpkcs12.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libplds4.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libssl.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libhw-acc-crypto.a" "${TARGET_ARCHS[@]}"
universal_lib "nss" "libgcm-aes-x86_c_lib.a" "x86_64"

echo "# Building openssl"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[${i}]}
  DIST_DIR=$(abspath "ios/${ARCH}/openssl")
  if [ -d "${DIST_DIR}" ]; then
    echo "${DIST_DIR} already exists. Skipping building openssl."
  else
    ./build-openssl-ios.sh "${OPENSSL_SRC_PATH}" "${DIST_DIR}" "${ARCH}" "${IOS_MIN_SDK_VERSION}" || exit 1
  fi
done
universal_lib "openssl" "libssl.a" "${TARGET_ARCHS[@]}"
universal_lib "openssl" "libcrypto.a" "${TARGET_ARCHS[@]}"

echo "# Building sqlcipher"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[${i}]}
  DIST_DIR=$(abspath "ios/${ARCH}/sqlcipher")
  if [ -d "${DIST_DIR}" ]; then
    echo "${DIST_DIR} already exists. Skipping building sqlcipher."
  else
    ./build-sqlcipher-ios.sh "${SQLCIPHER_SRC_PATH}" "${DIST_DIR}" "${ARCH}" "${IOS_MIN_SDK_VERSION}" || exit 1
  fi
done
universal_lib "sqlcipher" "libsqlcipher.a" "${TARGET_ARCHS[@]}"

HEADER_DIST_DIR="ios/universal/openssl/include/openssl"
if [ ! -e "${HEADER_DIST_DIR}" ]; then
  mkdir -p ${HEADER_DIST_DIR}
  cp -L "${OPENSSL_SRC_PATH}"/include/openssl/*.h "${HEADER_DIST_DIR}"
  # The following file is generated during compilation, we pick the one in arm64.
  cp -L "${PWD}"/ios/arm64/openssl/include/openssl/opensslconf.h "${HEADER_DIST_DIR}"
fi

HEADER_DIST_DIR="ios/universal/sqlcipher/include/sqlcipher"
if [ ! -e "${HEADER_DIST_DIR}" ]; then
  mkdir -p ${HEADER_DIST_DIR}
  # Choice of arm64 is arbitrary, it shouldn't matter.
  HEADER_SRC_DIR=$(abspath "ios/arm64/sqlcipher/include/sqlcipher")
  cp -L "${HEADER_SRC_DIR}"/*.h "${HEADER_DIST_DIR}"
fi

HEADER_DIST_DIR="ios/universal/nss/include/nss"
if [ ! -e "${HEADER_DIST_DIR}" ]; then
  mkdir -p ${HEADER_DIST_DIR}
  # Choice of arm64 is arbitrary, it shouldn't matter.
  HEADER_SRC_DIR=$(abspath "ios/arm64/nss/include/nss")
  cp -L "${HEADER_SRC_DIR}"/*.h "${HEADER_DIST_DIR}"
fi
