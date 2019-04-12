#!/usr/bin/env bash

set -euvx

OPENSSL_VERSION="1.1.1a"
OPENSSL_SHA256="fc20130f8b7cbd2fb918b2f14e2f429e109c31ddd0fb38fc5d71d9ffed3f9f41"

SQLCIPHER_VERSION="4.0.0"
SQLCIPHER_SHA256="c8f5fc6d800aae6107bf23900144804db5510c2676c93fbb269e4a0700837d68"

NSS="nss-3.43"
NSS_ARCHIVE="nss-3.43-with-nspr-4.21.tar.gz"
NSS_URL="http://ftp.mozilla.org/pub/security/nss/releases/NSS_3_43_RTM/src/${NSS_ARCHIVE}"
NSS_SHA256="fb2d54d507ceb185bac73f492cce7086a462d41977c2378aba9dd10e04448cf3"

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
if [ ! -e "${NSS_ARCHIVE}" ]; then
  echo "Downloading ${NSS_ARCHIVE}"
  curl -L -O "${NSS_URL}"
else
  echo "Using ${NSS_ARCHIVE}"
fi
echo "${NSS_SHA256}  ${NSS_ARCHIVE}" | shasum -a 256 -c - || exit 2
tar xfz "${NSS_ARCHIVE}"
NSS_SRC_PATH=$(abspath "${NSS}")
./patch-nss-src.sh "${NSS_SRC_PATH}"

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
