#!/bin/bash

# This script downloads and builds the Android jansson library.

set -e

if [ "$#" -ne 5 ]
then
    echo "Usage:"
    echo "./build-jansson-android.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <TOOLCHAIN_PATH> <TOOLCHAIN> <ANDROID_API_VERSION>"
    exit 1
fi

JANSSON_DIR=$1
DIST_DIR=$2
TOOLCHAIN_PATH=$3
TOOLCHAIN=$4
ANDROID_API_VERSION=$5

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

cd "${JANSSON_DIR}"

export TOOLCHAIN_BIN="$TOOLCHAIN_PATH""/bin/"
export CC="$TOOLCHAIN_BIN""$TOOLCHAIN""-gcc"
export CXX="$TOOLCHAIN_BIN""$TOOLCHAIN""-g++"
export RANLIB="$TOOLCHAIN_BIN""$TOOLCHAIN""-ranlib"
export LD="$TOOLCHAIN_BIN""$TOOLCHAIN""-ld"
export AR="$TOOLCHAIN_BIN""$TOOLCHAIN""-ar"
export CFLAGS="-D__ANDROID_API__=$ANDROID_API_VERSION"

make clean || true
./configure --host="$TOOLCHAIN" && make
mkdir -p "$DIST_DIR""/include"
mkdir -p "$DIST_DIR""/lib"
cp -p src/.libs/libjansson.so "$DIST_DIR""/lib"
cp "$PWD"/src/*.h "$DIST_DIR""/include"
