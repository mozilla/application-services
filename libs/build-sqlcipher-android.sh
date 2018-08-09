#!/bin/bash

# This script downloads and builds the SQLcipher library for Android.

set -euvx

if [ "$#" -ne 6 ]
then
    echo "Usage:"
    echo "./build-sqlcipher-android.sh <ABSOLUTE_SRC_DIR> <DIST_DIR> <TOOLCHAIN_PATH> <TOOLCHAIN> <ANDROID_NDK_API_VERSION> <OPENSSL_DIR>"
    exit 1
fi

SQLCIPHER_DIR=$1
DIST_DIR=$2
TOOLCHAIN_PATH=$3
TOOLCHAIN=$4
ANDROID_NDK_API_VERSION=$5
OPENSSL_DIR=$6

if [ -d "$DIST_DIR" ]; then
  echo "$DIST_DIR"" folder already exists. Skipping build."
  exit 0
fi

cd "${SQLCIPHER_DIR}"

export TOOLCHAIN_BIN="$TOOLCHAIN_PATH""/bin/"
export CC="$TOOLCHAIN_BIN""$TOOLCHAIN""-gcc"
export CXX="$TOOLCHAIN_BIN""$TOOLCHAIN""-g++"
export RANLIB="$TOOLCHAIN_BIN""$TOOLCHAIN""-ranlib"
export LD="$TOOLCHAIN_BIN""$TOOLCHAIN""-ld"
export AR="$TOOLCHAIN_BIN""$TOOLCHAIN""-ar"
export CFLAGS="-D__ANDROID_API__=$ANDROID_NDK_API_VERSION"

SQLCIPHER_OUTPUT_PATH="/tmp/sqlcipher-""$TOOLCHAIN"_$$
mkdir -p "$SQLCIPHER_OUTPUT_PATH"

if [ "$TOOLCHAIN" == "i686-linux-android" ]
then
  HOST="i686-linux"
elif [ "$TOOLCHAIN" == "aarch64-linux-android" ]
then
  HOST="arm-linux"
elif [ "$TOOLCHAIN" == "arm-linux-androideabi" ]
then
  HOST="arm-linux"
else
  echo "Unknown toolchain"
  exit 1
fi

SQLCIPHER_CFLAGS="-DSQLITE_HAS_CODEC -DSQLITE_SOUNDEX -DHAVE_USLEEP=1 -DSQLITE_MAX_VARIABLE_NUMBER=99999 -DSQLITE_TEMP_STORE=3 -DSQLITE_THREADSAFE=1 -DSQLITE_DEFAULT_JOURNAL_SIZE_LIMIT=1048576 -DNDEBUG=1 -DSQLITE_ENABLE_MEMORY_MANAGEMENT=1 -DSQLITE_ENABLE_LOAD_EXTENSION -DSQLITE_ENABLE_COLUMN_METADATA -DSQLITE_ENABLE_UNLOCK_NOTIFY -DSQLITE_ENABLE_RTREE -DSQLITE_ENABLE_STAT3 -DSQLITE_ENABLE_STAT4 -DSQLITE_ENABLE_JSON1 -DSQLITE_ENABLE_FTS3_PARENTHESIS -DSQLITE_ENABLE_FTS4 -DSQLITE_ENABLE_FTS5 -DSQLCIPHER_CRYPTO_OPENSSL -DSQLITE_ENABLE_DBSTAT_VTA"

make clean || true
./configure --host="${HOST}" --enable-tempstore=yes CFLAGS="${CFLAGS} ${SQLCIPHER_CFLAGS} -I${OPENSSL_DIR}/include -L${OPENSSL_DIR}/lib" LDFLAGS="-lcrypto -llog -lm" --prefix="${SQLCIPHER_OUTPUT_PATH}"
make
make install

mkdir -p "$DIST_DIR""/include/sqlcipher"
mkdir -p "$DIST_DIR""/lib"

# Turn libsqlcipher.so.0.8.6 into libsqlcipher.so.
REALLIB=`readlink "$SQLCIPHER_OUTPUT_PATH"/lib/libsqlcipher.so`
cp -p "$SQLCIPHER_OUTPUT_PATH"/lib/libsqlcipher.a "$DIST_DIR"/lib/libsqlcipher.a

# Just in case, ensure that the created binaries are not -w.
chmod +w "$DIST_DIR"/lib/libsqlcipher.a
rm -rf "$SQLCIPHER_OUTPUT_PATH"
