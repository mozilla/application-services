#!/bin/bash

set -e

IOS_MIN_SDK_VERSION="9.0"
TARGET_ARCHS=("i386" "x86_64" "armv7" "arm64")
TARGET_ARCHS_HOSTS=("i386-apple-darwin" "x86_64-apple-darwin" "armv7-apple-darwin" "arm-apple-darwin")

# End of configuration.

if [ "$#" -ne 3 ]
then
    echo "Usage:"
    echo "./build-all-ios.sh <JANSSON_SRC_PATH> <OPENSSL_SRC_PATH> <CJOSE_SRC_PATH>"
    exit 1
fi

JANSSON_SRC_PATH=$1
OPENSSL_SRC_PATH=$2
CJOSE_SRC_PATH=$3

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

echo "# Building jansson"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "ios/""$ARCH""/jansson")
  ./build-jansson-ios.sh "$JANSSON_SRC_PATH" "$DIST_DIR" "$ARCH" "${TARGET_ARCHS_HOSTS[$i]}" "$IOS_MIN_SDK_VERSION"
done
universal_lib "jansson" "libjansson.a"
HEADER_DIST_DIR="ios/universal/jansson/include"
if [ ! -e "$HEADER_DIST_DIR" ]; then
  mkdir -p $HEADER_DIST_DIR
  cp "$JANSSON_SRC_PATH"/src/*.h "$HEADER_DIST_DIR"
fi

echo "# Building openssl"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "ios/""$ARCH""/openssl")
  ./build-openssl-ios.sh "$OPENSSL_SRC_PATH" "$DIST_DIR" "$ARCH" "$IOS_MIN_SDK_VERSION"
done
universal_lib "openssl" "libssl.a"
universal_lib "openssl" "libcrypto.a"
HEADER_DIST_DIR="ios/universal/openssl/include/openssl"
if [ ! -e "$HEADER_DIST_DIR" ]; then
  mkdir -p $HEADER_DIST_DIR
  cp -L "$OPENSSL_SRC_PATH"/include/openssl/*.h "$HEADER_DIST_DIR"
fi

echo "# Building cjose"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "ios/""$ARCH""/cjose")
  JANSSON_DIR=$(abspath "ios/""$ARCH""/jansson")
  OPENSSL_DIR=$(abspath "ios/""$ARCH""/openssl")
  ./build-cjose-ios.sh "$CJOSE_SRC_PATH" "$DIST_DIR" "$ARCH" "${TARGET_ARCHS_HOSTS[$i]}" "$IOS_MIN_SDK_VERSION" "$JANSSON_DIR" "$OPENSSL_DIR"
done
universal_lib "cjose" "libcjose.a"
HEADER_DIST_DIR="ios/universal/cjose/include"
if [ ! -e "$HEADER_DIST_DIR" ]; then
  mkdir -p $HEADER_DIST_DIR
  cp "$CJOSE_SRC_PATH"/include/cjose/*.h "$HEADER_DIST_DIR"
fi
