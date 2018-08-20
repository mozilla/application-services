#!/usr/bin/env bash

# This script downloads and builds the Android openssl library.

set -euvx

if [ "$#" -ne 5 ]
then
    echo "Usage:"
    echo "./build-openssl-android.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <TOOLCHAIN_PATH> <TOOLCHAIN> <ANDROID_NDK_API_VERSION>"
    exit 1
fi

OPENSSL_DIR=$1
DIST_DIR=$2
TOOLCHAIN_PATH=$3
TOOLCHAIN=$4
ANDROID_NDK_API_VERSION=$5

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

cd "${OPENSSL_DIR}"

export TOOLCHAIN_BIN="$TOOLCHAIN_PATH""/bin/"
export CC="$TOOLCHAIN_BIN""$TOOLCHAIN""-gcc"
export CXX="$TOOLCHAIN_BIN""$TOOLCHAIN""-g++"
export RANLIB="$TOOLCHAIN_BIN""$TOOLCHAIN""-ranlib"
export LD="$TOOLCHAIN_BIN""$TOOLCHAIN""-ld"
export AR="$TOOLCHAIN_BIN""$TOOLCHAIN""-ar"
export CFLAGS="-D__ANDROID_API__=$ANDROID_NDK_API_VERSION"

OPENSSL_OUTPUT_PATH="/tmp/openssl-""$TOOLCHAIN"_$$
mkdir -p "$OPENSSL_OUTPUT_PATH"

if [ "$TOOLCHAIN" == "i686-linux-android" ]
then
  CONFIGURE_ARCH="android-x86"
elif [ "$TOOLCHAIN" == "aarch64-linux-android" ]
then
  CONFIGURE_ARCH="android"
elif [ "$TOOLCHAIN" == "arm-linux-androideabi" ]
then
  CONFIGURE_ARCH="android"
else
  echo "Unknown toolchain"
  exit 1
fi

make clean || true
./Configure "$CONFIGURE_ARCH" shared --openssldir="$OPENSSL_OUTPUT_PATH"
make -j6
make install_sw
mkdir -p "$DIST_DIR""/include/openssl"
mkdir -p "$DIST_DIR""/lib"
cp -p "$OPENSSL_OUTPUT_PATH"/lib/libssl.a "$DIST_DIR"/lib
cp -p "$OPENSSL_OUTPUT_PATH"/lib/libcrypto.a "$DIST_DIR"/lib
cp -L "$PWD"/include/openssl/*.h "${DIST_DIR}/include/openssl"
# For some reason the created binaries are -w.
chmod +w "$DIST_DIR"/lib/libssl.a
chmod +w "$DIST_DIR"/lib/libcrypto.a
rm -rf "$OPENSSL_OUTPUT_PATH"
