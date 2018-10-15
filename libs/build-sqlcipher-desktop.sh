#!/usr/bin/env bash

set -euvx

if [ "$#" -ne 2 ]
then
  echo "Usage:"
  echo "./build-sqlcipher-desktop.sh <SQLCIPHER_SRC_PATH> <OPENSSL_DIR>"
  exit 1
fi

SQLCIPHER_SRC_PATH=$1
OPENSSL_DIR=$2
SQLCIPHER_DIR=$(abspath "desktop/sqlcipher")

if [ -d "$SQLCIPHER_DIR" ]; then
  echo "$SQLCIPHER_DIR folder already exists. Skipping build."
  exit 0
fi

echo "# Building sqlcipher"

# Keep in sync with SQLCIPHER_CFLAGS in `build-sqlcipher-desktop.sh` for now (it probably makes
# sense to try to avoid this duplication in the future).
# TODO: We could probably prune some of these, and it would be nice to allow debug builds (which
# should set `SQLITE_DEBUG` and `SQLITE_ENABLE_API_ARMOR` and not `NDEBUG`).
SQLCIPHER_CFLAGS=" \
  -DSQLITE_HAS_CODEC \
  -DSQLITE_SOUNDEX \
  -DHAVE_USLEEP=1 \
  -DSQLITE_MAX_VARIABLE_NUMBER=99999 \
  -DSQLITE_TEMP_STORE=3 \
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
  -DSQLCIPHER_CRYPTO_OPENSSL \
  -DSQLITE_ENABLE_DBSTAT_VTAB \
  -DSQLITE_SECURE_DELETE=1 \
  -DSQLITE_DEFAULT_PAGE_SIZE=32768 \
  -DSQLITE_MAX_DEFAULT_PAGE_SIZE=32768 \
"

rm -rf "$SQLCIPHER_SRC_PATH/build-desktop"
mkdir -p "$SQLCIPHER_SRC_PATH/build-desktop/install-prefix"
pushd "$SQLCIPHER_SRC_PATH/build-desktop"

../configure --prefix="$PWD/install-prefix" \
  --enable-tempstore=yes \
  CFLAGS="$SQLCIPHER_CFLAGS -I$OPENSSL_DIR/include -L$OPENSSL_DIR/lib" \
  LDFLAGS="-lcrypto -lm"

make -j6 && make install

mkdir -p "$SQLCIPHER_DIR/lib"
cp -r "install-prefix/include" "$SQLCIPHER_DIR"
cp -p "install-prefix/lib/libsqlcipher.a" "$SQLCIPHER_DIR/lib/libsqlcipher.a"

chmod +w "$SQLCIPHER_DIR/lib/libsqlcipher.a"

popd
