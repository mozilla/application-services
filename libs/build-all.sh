#!/usr/bin/env bash

set -euvx

OPENSSL_VERSION="1.1.1a"
OPENSSL_SHA256="fc20130f8b7cbd2fb918b2f14e2f429e109c31ddd0fb38fc5d71d9ffed3f9f41"

SQLCIPHER_VERSION="4.1.0"
SQLCIPHER_SHA256="65144ca3ba4c0f9cd4bae8c20bb42f2b84424bf29d1ebcf04c44a728903b1faa"

NSS="nss-3.44"
NSS_ARCHIVE="nss-3.44-with-nspr-4.21.tar.gz"
NSS_URL="http://ftp.mozilla.org/pub/security/nss/releases/NSS_3_44_RTM/src/${NSS_ARCHIVE}"
NSS_SHA256="298d86e18e96660d3c98476274b5857b48c135d809a10d6528d8661bdf834a49"

# End of configuration.

if [ "${#}" -ne 1 ]
then
    echo "Usage:"
    echo "./build-all.sh [ios|android|desktop]"
    exit 1
fi

PLATFORM="${1}"

abspath () { case "${1}" in /*)printf "%s\\n" "${1}";; *)printf "%s\\n" "${PWD}/${1}";; esac; }
export -f abspath

OPENSSL="openssl-${OPENSSL_VERSION}"
rm -rf "${OPENSSL}"
if [ ! -e "${OPENSSL}.tar.gz" ]; then
  echo "Downloading ${OPENSSL}.tar.gz"
  curl -L -O "https://www.openssl.org/source/${OPENSSL}.tar.gz"
else
  echo "Using ${OPENSSL}.tar.gz"
fi
echo "${OPENSSL_SHA256}  ${OPENSSL}.tar.gz" | shasum -a 256 -c - || exit 2
tar xfz "${OPENSSL}.tar.gz"
OPENSSL_SRC_PATH=$(abspath ${OPENSSL})

SQLCIPHER="v${SQLCIPHER_VERSION}"
rm -rf "${SQLCIPHER}"
if [ ! -e "${SQLCIPHER}.tar.gz" ]; then
  echo "Downloading ${SQLCIPHER}.tar.gz"
  curl -L -O "https://github.com/sqlcipher/sqlcipher/archive/${SQLCIPHER}.tar.gz"
else
  echo "Using ${SQLCIPHER}.tar.gz"
fi
echo "${SQLCIPHER_SHA256}  ${SQLCIPHER}.tar.gz" | shasum -a 256 -c - || exit 2
tar xfz "${SQLCIPHER}.tar.gz"
SQLCIPHER_SRC_PATH=$(abspath "sqlcipher-${SQLCIPHER_VERSION}")

rm -rf "${NSS}"
# Delete the following...
hg clone https://hg.mozilla.org/projects/nss/ -r 65efa74ef84a3b2fcab7fc960ee7c05e28bab2b1 "${NSS}"/nss
hg clone https://hg.mozilla.org/projects/nspr/ -r 87e3d40f7fef5b67a28d6160a731a2d3118078be "${NSS}"/nspr
# ... and uncomment the following once NSS 3.45 and NSPR 4.22 are out.
# if [ ! -e "${NSS_ARCHIVE}" ]; then
#   echo "Downloading ${NSS_ARCHIVE}"
#   curl -L -O "${NSS_URL}"
# else
#   echo "Using ${NSS_ARCHIVE}"
# fi
# echo "${NSS_SHA256}  ${NSS_ARCHIVE}" | shasum -a 256 -c - || exit 2
# tar xfz "${NSS_ARCHIVE}"
NSS_SRC_PATH=$(abspath "${NSS}")

if [ "${PLATFORM}" == "ios" ]
then
  ./build-all-ios.sh "${OPENSSL_SRC_PATH}" "${SQLCIPHER_SRC_PATH}" "${NSS_SRC_PATH}"
elif [ "${PLATFORM}" == "android" ]
then
  ./build-all-android.sh "${OPENSSL_SRC_PATH}" "${SQLCIPHER_SRC_PATH}" "${NSS_SRC_PATH}"
elif [ "${PLATFORM}" == "desktop" ]
then
  ./build-nss-desktop.sh "${NSS_SRC_PATH}"
  ./build-openssl-desktop.sh "${OPENSSL_SRC_PATH}"
  ./build-sqlcipher-desktop.sh "${SQLCIPHER_SRC_PATH}"
elif [ "${PLATFORM}" == "darwin" -o "${PLATFORM}" == "win32-x86-64" ]
then
  ./build-nss-desktop.sh "${NSS_SRC_PATH}" "${PLATFORM}"
  ./build-openssl-desktop.sh "${OPENSSL_SRC_PATH}" "${PLATFORM}"
  ./build-sqlcipher-desktop.sh "${SQLCIPHER_SRC_PATH}" "${PLATFORM}"
else
  echo "Unrecognized platform"
  exit 1
fi

echo "Cleaning up"
rm -rf "${OPENSSL_SRC_PATH}"
rm -rf "${SQLCIPHER_SRC_PATH}"
rm -rf "${NSS_SRC_PATH}"

echo "Done"
