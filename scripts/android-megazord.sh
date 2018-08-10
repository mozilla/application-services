#!/usr/bin/env bash

set -evx

TARGET=$1
TRIPLE=""
if [ -z "$TARGET" ]; then
    echo "Usage: $0 (x86|arm|arm64) [debug|release = debug] [workspace-relative-crate-dir = ffi-megazord]"
    exit 1
fi

TRIPLE=""
case $TARGET in
x86)
    TRIPLE="i686-linux-android"
    ;;
arm)
    TRIPLE="armv7-linux-androideabi"
    ;;
arm64)
    TRIPLE="aarch64-linux-android"
    ;;
*)
    echo "Unknown Target: '$TARGET'. Must be x86/arm/arm64"
    exit 1
esac

BUILD_TYPE=$2
if [ -z "$BUILD_TYPE" ]; then
    BUILD_TYPE=debug
fi

# XXX this script is abused by `
ROOT_REL_LIB_PATH=$3
if [ -z "$ROOT_REL_LIB_PATH" ]; then
    ROOT_REL_LIB_PATH=ffi-megazord
fi

if [ ! -f libs/android/$TARGET/sqlcipher/lib/libsqlcipher.a ]; then
    echo "Error: no static lib of libsqlcipher (or probably openssl)."
    echo "  You probably want to erase libs/android and then run ./libs/build-all.sh android"
    echo "  before trying this again."
    exit 1
fi

APPSVC_ROOT="$PWD"

cd $ROOT_REL_LIB_PATH

CARGO_ARGS="--target $TRIPLE --verbose"
if [ "$BUILD_TYPE" = "release" ]; then
    CARGO_ARGS="$CARGO_ARGS --release"
fi

env PATH="$PATH:$ANDROID_NDK_TOOLCHAIN_DIR/$TARGET-$ANDROID_NDK_API_VERSION/bin" \
    SQLCIPHER_INCLUDE_DIR=$APPSVC_ROOT/libs/android/$TARGET/sqlcipher/include \
    SQLCIPHER_LIB_DIR=$APPSVC_ROOT/libs/android/$TARGET/sqlcipher/lib \
    OPENSSL_INCLUDE_DIR=$APPSVC_ROOT/libs/android/$TARGET/openssl/include \
    OPENSSL_LIB_DIR=$APPSVC_ROOT/libs/android/$TARGET/openssl/lib \
    OPENSSL_STATIC=1 \
    cargo build $CARGO_ARGS

cd -
