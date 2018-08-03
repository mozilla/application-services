#!/bin/bash

set -euvx

OPENSSL_VERSION="1.0.2o"

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
tar xfz "${OPENSSL}.tar.gz"
OPENSSL_SRC_PATH=$(abspath $OPENSSL)

if [ "$PLATFORM" == "ios" ]
then
  ./build-all-ios.sh "$OPENSSL_SRC_PATH"
elif [ "$PLATFORM" == "android" ]
then
  ./build-all-android.sh "$OPENSSL_SRC_PATH"
elif [ "$PLATFORM" == "desktop" ]
then
  ./build-all-desktop.sh "$OPENSSL_SRC_PATH"
else
  echo "Unrecognized platform"
  exit 1
fi

echo "Cleaning up"
rm -rf "$OPENSSL_SRC_PATH"

echo "Done"
