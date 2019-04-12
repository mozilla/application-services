#!/usr/bin/env bash

# This script downloads and builds the SQLcipher library for iOS.

set -euvx

if [ "${#}" -ne 4 ]
then
    echo "Usage:"
    echo "./build-sqlcipher-ios.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <ARCH> <IOS_MIN_SDK_VERSION>"
    exit 1
fi

SQLCIPHER_SRC_DIR=${1}
DIST_DIR=${2}
ARCH=${3}
IOS_MIN_SDK_VERSION=${4}

if [ -d "${DIST_DIR}" ]; then
  echo "${DIST_DIR}"" folder already exists. Skipping build."
  exit 0
fi

SQLCIPHER_IOS="${SQLCIPHER_SRC_DIR}/build-ios-""${ARCH}_${$}"
mkdir -p "${SQLCIPHER_IOS}"
pushd "${SQLCIPHER_IOS}"

if [[ "${ARCH}" == "x86_64" ]]; then
  OS_COMPILER="iPhoneSimulator"
  HOST="x86_64-apple-darwin"
elif [[ "${ARCH}" == "arm64" ]]; then
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

# Match the SQLCIPHER_CFLAGS used in Firefox for iOS v15.x and earlier.
# NOTE: iOS v15.x and earlier used -DSQLITE_THREADSAFE=2, but the
# SQLCipher `configure` script seems to overwrite it to only be 0 or 1.
SQLCIPHER_CFLAGS=" \
  -DNDEBUG=1 \
  -DSQLITE_HAS_CODEC \
  -DSQLITE_THREADSAFE=1 \
  -DSQLITE_TEMP_STORE=2 \
  -DSQLITE_MAX_VARIABLE_NUMBER=99999 \
  -DSQLITE_ENABLE_JSON1 \
  -DSQLITE_ENABLE_FTS3 \
  -DSQLITE_ENABLE_FTS3_PARENTHESIS \
  -DSQLITE_ENABLE_FTS4 \
  -DSQLITE_ENABLE_FTS5 \
"
# These additional options are used on desktop, but are not currently
# used on iOS until we can performance test them:
#   -DSQLITE_SOUNDEX \
#   -DHAVE_USLEEP=1 \
#   -DSQLITE_ENABLE_MEMORY_MANAGEMENT=1 \
#   -DSQLITE_ENABLE_LOAD_EXTENSION \
#   -DSQLITE_ENABLE_COLUMN_METADATA \
#   -DSQLITE_ENABLE_UNLOCK_NOTIFY \
#   -DSQLITE_ENABLE_RTREE \
#   -DSQLITE_ENABLE_STAT3 \
#   -DSQLITE_ENABLE_STAT4 \
#   -DSQLITE_ENABLE_DBSTAT_VTAB \
#   -DSQLITE_DEFAULT_JOURNAL_SIZE_LIMIT=1048576 \
#   -DSQLITE_DEFAULT_PAGE_SIZE=32768 \
#   -DSQLITE_MAX_DEFAULT_PAGE_SIZE=32768 \

# One additional option is used on desktop that has a known
# performance penalty. However, it can be enabled per-connection
# at runtime with `PRAGMA secure_delete`:
#   -DSQLITE_SECURE_DELETE \

../configure \
  --with-pic \
  --disable-tcl \
  --host="${HOST}" \
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

mkdir -p "${DIST_DIR}/include/sqlcipher"
mkdir -p "${DIST_DIR}/lib"

cp -p "${SQLCIPHER_IOS}/sqlite3.h" "${DIST_DIR}/include/sqlcipher"
cp -p "${SQLCIPHER_IOS}/sqlite3ext.h" "${DIST_DIR}/include/sqlcipher"
cp -p "${SQLCIPHER_IOS}/.libs/libsqlcipher.a" "${DIST_DIR}/lib"

popd

rm -rf ${SQLCIPHER_IOS}
