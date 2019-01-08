#!/usr/bin/env bash

# This script downloads and builds the SQLcipher library for iOS.

set -euvx

if [ "$#" -ne 4 ]
then
    echo "Usage:"
    echo "./build-sqlcipher-ios.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <IOS_MIN_SDK_VERSION>"
    exit 1
fi

SQLCIPHER_SRC_DIR=$1
DIST_DIR=$2
ARCH=$3
IOS_MIN_SDK_VERSION=$4

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

SQLCIPHER_IOS="$SQLCIPHER_SRC_DIR/build-ios-""$ARCH"_$$
mkdir -p "$SQLCIPHER_IOS"
pushd "$SQLCIPHER_IOS"

if [[ "${ARCH}" == "i386" || "${ARCH}" == "x86_64" ]]; then
  OS_COMPILER="iPhoneSimulator"
  if [[ "${ARCH}" == "x86_64" ]]; then
    HOST="x86_64-apple-darwin"
  else
    HOST="x86-apple-darwin"
  fi
elif [[ "${ARCH}" == "armv7" || "${ARCH}" == "arm64" ]]; then
  OS_COMPILER="iPhoneOS"
  HOST="arm-apple-darwin"
else
  echo "Unsupported architecture"
  exit 1
fi

DEVELOPER=$(xcode-select -print-path)
export CROSS_TOP="${DEVELOPER}/Platforms/${OS_COMPILER}.platform/Developer"
export CROSS_SDK="${OS_COMPILER}.sdk"
TOOLCHAIN_BIN="${DEVELOPER}/Toolchains/XcodeDefault.xctoolchain/usr/bin"
export CC="${TOOLCHAIN_BIN}/clang"
export AR="${TOOLCHAIN_BIN}/ar"
export RANLIB="${TOOLCHAIN_BIN}/ranlib"
export STRIP="${TOOLCHAIN_BIN}/strip"
export LIBTOOL="${TOOLCHAIN_BIN}/libtool"
export NM="${TOOLCHAIN_BIN}/nm"
export LD="${TOOLCHAIN_BIN}/ld"

CFLAGS="\
  -fembed-bitcode \
  -arch ${ARCH} \
  -isysroot ${CROSS_TOP}/SDKs/${CROSS_SDK} \
  -mios-version-min=${IOS_MIN_SDK_VERSION} \
"

# Keep in sync with SQLCIPHER_CFLAGS in `build-sqlcipher-desktop.sh` for now (it probably makes
# sense to try to avoid this duplication in the future).
# TODO: We could probably prune some of these, and it would be nice to allow debug builds (which
# should set `SQLITE_DEBUG` and `SQLITE_ENABLE_API_ARMOR` and not `NDEBUG`).
SQLCIPHER_CFLAGS=" \
  -DSQLITE_HAS_CODEC \
  -DSQLITE_SOUNDEX \
  -DHAVE_USLEEP=1 \
  -DSQLITE_MAX_VARIABLE_NUMBER=99999 \
  -DSQLITE_THREADSAFE=1 \
  -DSQLITE_DEFAULT_JOURNAL_SIZE_LIMIT=1048576 \
  -DNDEBUG=1 \
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
  -DSQLITE_ENABLE_DBSTAT_VTAB \
  -DSQLITE_SECURE_DELETE \
  -DSQLITE_DEFAULT_PAGE_SIZE=32768 \
  -DSQLITE_MAX_DEFAULT_PAGE_SIZE=32768 \
"

../configure \
  --with-pic \
  --disable-tcl \
  --host="$HOST" \
  --verbose \
  --with-crypto-lib=commoncrypto \
  --enable-tempstore=yes \
  --enable-threadsafe=yes \
  --disable-editline \
  CFLAGS="${CFLAGS} ${SQLCIPHER_CFLAGS}" \
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
