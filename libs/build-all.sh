#!/bin/bash

set -e

JANSSON_VERSION="2.11"
OPENSSL_VERSION="1.0.2o"
CJOSE_VERSION="0.6.1"

# End of configuration.

if [ "$#" -ne 1 ]
then
    echo "Usage:"
    echo "./build-all.sh [ios|android]"
    exit 1
fi

PLATFORM=$1

abspath () { case "$1" in /*)printf "%s\\n" "$1";; *)printf "%s\\n" "$PWD/$1";; esac; }
export -f abspath

JANSSON="jansson-${JANSSON_VERSION}"
rm -rf "${JANSSON}"
if [ ! -e "${JANSSON}.tar.gz" ]; then
  echo "Downloading ${JANSSON}.tar.gz"
  curl -O "http://www.digip.org/jansson/releases/${JANSSON}.tar.gz"
else
  echo "Using ${JANSSON}.tar.gz"
fi
tar xfz "${JANSSON}.tar.gz"
JANSSON_SRC_PATH=$(abspath $JANSSON)

OPENSSL="openssl-${OPENSSL_VERSION}"
rm -rf "${OPENSSL}"
if [ ! -e "${OPENSSL}.tar.gz" ]; then
  echo "Downloading ${OPENSSL}.tar.gz"
  curl -O "https://www.openssl.org/source/""${OPENSSL}"".tar.gz"
else
  echo "Using ${OPENSSL}.tar.gz"
fi
tar xfz "${OPENSSL}.tar.gz"
OPENSSL_SRC_PATH=$(abspath $OPENSSL)

CJOSE="cjose-${CJOSE_VERSION}"
rm -rf "${CJOSE}"
if [ ! -e "${CJOSE}.tar.gz" ]; then
  echo "Downloading ${CJOSE}.tar.gz"
  curl -L "https://github.com/cisco/cjose/archive/${CJOSE_VERSION}.tar.gz" -o "${CJOSE}.tar.gz"
else
  echo "Using ${CJOSE}.tar.gz"
fi
tar xfz "${CJOSE}.tar.gz"
CJOSE_SRC_PATH=$(abspath $CJOSE)

if [ "$PLATFORM" == "ios" ]
then
  ./build-all-ios.sh "$JANSSON_SRC_PATH" "$OPENSSL_SRC_PATH" "$CJOSE_SRC_PATH"
elif [ "$PLATFORM" == "android" ]
then
  ./build-all-android.sh "$JANSSON_SRC_PATH" "$OPENSSL_SRC_PATH" "$CJOSE_SRC_PATH"
else
  echo "Unrecognized platform"
  exit 1
fi

echo "Cleaning up"
rm -rf "$CJOSE_SRC_PATH"
rm -rf "$JANSSON_SRC_PATH"
rm -rf "$OPENSSL_SRC_PATH"

echo "Done"
