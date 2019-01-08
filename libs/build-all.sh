#!/usr/bin/env bash

set -euvx

OPENSSL_VERSION="1.1.1a"
OPENSSL_SHA256="fc20130f8b7cbd2fb918b2f14e2f429e109c31ddd0fb38fc5d71d9ffed3f9f41"

SQLCIPHER_VERSION="4.0.0"
SQLCIPHER_SHA256="c8f5fc6d800aae6107bf23900144804db5510c2676c93fbb269e4a0700837d68"

# End of configuration.

if [ "$#" -ne 1 ]
then
    echo "Usage:"
    echo "./build-all.sh [ios|android|desktop]"
    exit 1
fi

PLATFORM=$1

abspath () { case "$1" in /*)printf "%s\\n" "$1";; *)printf "%s\\n" "$PWD/$1";; esac; }
export -f abspath

OPENSSL="openssl-${OPENSSL_VERSION}"
rm -rf "${OPENSSL}"
if [ ! -e "${OPENSSL}.tar.gz" ]; then
  echo "Downloading ${OPENSSL}.tar.gz"
  curl -L -O "https://www.openssl.org/source/""${OPENSSL}"".tar.gz"
else
  echo "Using ${OPENSSL}.tar.gz"
fi

echo "${OPENSSL_SHA256}  ${OPENSSL}.tar.gz" | shasum -a 256 -c - || exit 2

tar xfz "${OPENSSL}.tar.gz"
OPENSSL_SRC_PATH=$(abspath $OPENSSL)


SQLCIPHER="v${SQLCIPHER_VERSION}"
rm -rf "${SQLCIPHER}"
if [ ! -e "${SQLCIPHER}.tar.gz" ]; then
  echo "Downloading ${SQLCIPHER}.tar.gz"
  curl -L -O "https://github.com/sqlcipher/sqlcipher/archive/""${SQLCIPHER}"".tar.gz"
else
  echo "Using ${SQLCIPHER}.tar.gz"
fi

echo "${SQLCIPHER_SHA256}  ${SQLCIPHER}.tar.gz" | shasum -a 256 -c - || exit 2

tar xfz "${SQLCIPHER}.tar.gz"
SQLCIPHER_SRC_PATH=$(abspath "sqlcipher-${SQLCIPHER_VERSION}")

if [ "$PLATFORM" == "ios" ]
then
  ./build-all-ios.sh "$OPENSSL_SRC_PATH" "$SQLCIPHER_SRC_PATH"
elif [ "$PLATFORM" == "android" ]
then
  ./build-all-android.sh "$OPENSSL_SRC_PATH" "$SQLCIPHER_SRC_PATH"
elif [ "$PLATFORM" == "desktop" ]
then
  ./build-openssl-desktop.sh "$OPENSSL_SRC_PATH"
  ./build-sqlcipher-desktop.sh "$SQLCIPHER_SRC_PATH"
elif [ "$PLATFORM" == "darwin" -o "$PLATFORM" == "win32-x86-64" ]
then
  ./build-openssl-desktop.sh "$OPENSSL_SRC_PATH" "$PLATFORM"
  ./build-sqlcipher-desktop.sh "$SQLCIPHER_SRC_PATH" "$PLATFORM"
else
  echo "Unrecognized platform"
  exit 1
fi

echo "Cleaning up"
rm -rf "$OPENSSL_SRC_PATH"
rm -rf "$SQLCIPHER_SRC_PATH"

echo "Done"
