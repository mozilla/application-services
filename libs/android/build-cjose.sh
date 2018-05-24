#!/bin/bash

# This script downloads and builds the Android cjose library.

set -e

if [ "$#" -ne 7 ]
then
    echo "Usage:"
    echo "./build-cjose.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <TOOLCHAIN_PATH> <TOOLCHAIN> <ANDROID_API_VERSION> <JANSSON_DIR> <OPENSSL_DIR>"
    exit 1
fi

CJOSE_DIR=$1
DIST_DIR=$2
TOOLCHAIN_PATH=$3
TOOLCHAIN=$4
ANDROID_API_VERSION=$5
JANSSON_DIR=$6
OPENSSL_DIR=$7

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

cd "${CJOSE_DIR}"

export TOOLCHAIN_BIN="$TOOLCHAIN_PATH""/bin/"
export CC="$TOOLCHAIN_BIN""$TOOLCHAIN""-gcc"
export CXX="$TOOLCHAIN_BIN""$TOOLCHAIN""-g++"
export RANLIB="$TOOLCHAIN_BIN""$TOOLCHAIN""-ranlib"
export LD="$TOOLCHAIN_BIN""$TOOLCHAIN""-ld"
export AR="$TOOLCHAIN_BIN""$TOOLCHAIN""-ar"
export CFLAGS="-D__ANDROID_API__=$ANDROID_API_VERSION"

make clean || true
./configure --host=${TOOLCHAIN} --with-openssl="${OPENSSL_DIR}" --with-jansson="${JANSSON_DIR}" && make
mkdir -p "$DIST_DIR""/include"
mkdir -p "$DIST_DIR""/lib"
cp -p src/.libs/libcjose.so "$DIST_DIR""/lib"
cp "$PWD"/include/cjose/*.h "$DIST_DIR""/include"
