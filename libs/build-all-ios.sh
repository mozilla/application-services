#!/usr/bin/env bash

set -e

IOS_MIN_SDK_VERSION="9.0"
TARGET_ARCHS=("i386" "x86_64" "armv7" "arm64")
TARGET_ARCHS_HOSTS=("i386-apple-darwin" "x86_64-apple-darwin" "armv7-apple-darwin" "arm-apple-darwin")

# End of configuration.

if [ "$#" -ne 2 ]
then
    echo "Usage:"
    echo "./build-all-ios.sh <OPENSSL_SRC_PATH> <SQLCIPHER_SRC_PATH>"
    exit 1
fi

OPENSSL_SRC_PATH=$1
SQLCIPHER_SRC_PATH=$2

function universal_lib() {
  DIR_NAME=$1
  LIB_NAME=$2
  UNIVERSAL_DIR="ios/universal/""$DIR_NAME"
  LIB_PATH=$UNIVERSAL_DIR"/lib/"$LIB_NAME
  if [ ! -e "$LIB_PATH" ]; then
    mkdir -p "$UNIVERSAL_DIR""/lib"
    CMD="lipo"
    for ARCH in "${TARGET_ARCHS[@]}"; do
      CMD="$CMD"" -arch ""$ARCH"" ios/""$ARCH""/""$DIR_NAME""/lib/""$LIB_NAME"
    done
    CMD="$CMD"" -output ""$LIB_PATH"" -create"
    ${CMD}
  fi
}

if [ -d "ios" ]; then
  echo "ios folder already exists. Skipping build."
  exit 0
fi

echo "# Building openssl"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "ios/""$ARCH""/openssl")
  ./build-openssl-ios.sh "$OPENSSL_SRC_PATH" "$DIST_DIR" "$ARCH" "$IOS_MIN_SDK_VERSION"
  DIST_DIR=$(abspath "ios/""$ARCH""/sqlcipher")
  HOST=${TARGET_ARCHS_HOSTS[$i]}
  ./build-sqlcipher-ios.sh "$SQLCIPHER_SRC_PATH" "$DIST_DIR" "$ARCH" "$HOST" "$IOS_MIN_SDK_VERSION"
done
universal_lib "openssl" "libssl.a"
universal_lib "openssl" "libcrypto.a"
universal_lib "sqlcipher" "libsqlcipher.a"

HEADER_DIST_DIR="ios/universal/openssl/include/openssl"
if [ ! -e "$HEADER_DIST_DIR" ]; then
  mkdir -p $HEADER_DIST_DIR
  cp -L "$OPENSSL_SRC_PATH"/include/openssl/*.h "$HEADER_DIST_DIR"
fi

HEADER_DIST_DIR="ios/universal/sqlcipher/include/sqlcipher"
if [ ! -e "$HEADER_DIST_DIR" ]; then
  mkdir -p $HEADER_DIST_DIR
  # Choice of arm64 is arbitrary, it shouldn't matter.
  HEADER_SRC_DIR=$(abspath "ios/arm64/sqlcipher/include/sqlcipher")
  cp -L "$HEADER_SRC_DIR"/*.h "$HEADER_DIST_DIR"
fi
