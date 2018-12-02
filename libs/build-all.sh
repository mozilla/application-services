#!/usr/bin/env bash

set -euvx

OPENSSL_VERSION="1.0.2p"
OPENSSL_SHA256="50a98e07b1a89eb8f6a99477f262df71c6fa7bef77df4dc83025a2845c827d00"

SQLCIPHER_VERSION="3.4.2"
SQLCIPHER_SHA256="69897a5167f34e8a84c7069f1b283aba88cdfa8ec183165c4a5da2c816cfaadb"

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
  # TODO: "$SQLCIPHER_SRC_PATH"
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
