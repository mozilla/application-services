#!/usr/bin/env bash

set -euvx

if [ "$#" -ne 5 ]
then
    echo "Usage:"
    echo "./build-sqlcipher-ios.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <HOST> <IOS_MIN_SDK_VERSION>"
    exit 1
fi

SQLCIPHER_SRC_DIR=$1
DIST_DIR=$2
ARCH=$3
HOST=$4
IOS_MIN_SDK_VERSION=$5

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

SQLCIPHER_IOS="$SQLCIPHER_SRC_DIR/build-ios-""$ARCH"_$$
mkdir -p "$SQLCIPHER_IOS"
pushd "$SQLCIPHER_IOS"

if [[ "${ARCH}" == "i386" || "${ARCH}" == "x86_64" ]]; then
  PLATFORM="iPhoneSimulator"
else
  PLATFORM="iPhoneOS"
fi

DEVELOPER=$(xcode-select -print-path)
export PLATFORM="$PLATFORM"
export CROSS_TOP="${DEVELOPER}/Platforms/${PLATFORM}.platform/Developer"
export CROSS_SDK="${PLATFORM}.sdk"
export BUILD_TOOLS="${DEVELOPER}"
export CC="$BUILD_TOOLS/usr/bin/gcc"
export LD="$BUILD_TOOLS/usr/bin/ld"
#
CFLAGS="\
  -fembed-bitcode \
  -arch ${ARCH} \
  -isysroot ${CROSS_TOP}/SDKs/${CROSS_SDK} \
  -miphoneos-version-min=${IOS_MIN_SDK_VERSION} \
"

if [[ "$ARCH" == "armv7" ]]; then
  CFLAGS="$CFLAGS -mno-thumb"
fi

SQLCIPHER_DEFINES=" \
  -DSQLITE_HAS_CODEC \
  -DSQLITE_ENABLE_MEMORY_MANAGEMENT=1 \
  -DSQLITE_ENABLE_LOAD_EXTENSION \
  -DSQLITE_ENABLE_COLUMN_METADATA \
  -DSQLITE_ENABLE_UNLOCK_NOTIFY \
  -DSQLITE_ENABLE_RTREE \
  -DSQLITE_ENABLE_STAT3 \
  -DSQLITE_ENABLE_STAT4 \
  -DSQLITE_ENABLE_JSON1 \
  -DSQLITE_ENABLE_FTS3_PARENTHESIS \
  -DSQLITE_ENABLE_FTS4 \
  -DSQLITE_ENABLE_FTS5 \
  -DSQLITE_ENABLE_DBSTAT_VTA \
"

../configure \
  --host="$HOST" \
  --verbose \
  --with-crypto-lib=commoncrypto \
  --enable-tempstore=yes \
  --enable-threadsafe=yes \
  --disable-editline \
  CFLAGS="$CFLAGS $SQLCIPHER_DEFINES" \
  LDFLAGS="-framework Security -framework Foundation"

# Make all fails because it tries to build the command line program.
# Can't find a way around this so we'll build what we need... Sort of.
# AFAICT there's no target in this makefile for `libsqlcipher.a`
# directly. `libsqlcipher.la` is a text file with info about `libsqlcipher.a`
# and has a target, so we build that, then steal libsqlcipher.a from
# the .libs folder (which autotools uses to store libraries created during
# the build process).

make sqlite3.h
make sqlite3ext.h
make libsqlcipher.la

mkdir -p "$DIST_DIR/include/sqlcipher"
mkdir -p "$DIST_DIR/lib"

cp -p "$SQLCIPHER_IOS/sqlite3.h" "$DIST_DIR/include/sqlcipher"
cp -p "$SQLCIPHER_IOS/sqlite3ext.h" "$DIST_DIR/include/sqlcipher"
cp -p "$SQLCIPHER_IOS/.libs/libsqlcipher.a" "$DIST_DIR/lib"

popd

rm -rf $SQLCIPHER_IOS
