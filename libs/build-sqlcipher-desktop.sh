#!/usr/bin/env bash

set -euvx

abspath () { case "$1" in /*)printf "%s\\n" "$1";; *)printf "%s\\n" "$PWD/$1";; esac; }
export -f abspath

if [ "$#" -lt 1 -o "$#" -gt 2 -o ]
then
  echo "Usage:"
  echo "./build-sqlcipher-desktop.sh <SQLCIPHER_SRC_PATH> [CROSS_COMPILE_MACOS]"
  exit 1
fi

SQLCIPHER_SRC_PATH=$1
# Whether to cross compile from Linux to macOS.  Really only intended
# for automation.
CROSS_COMPILE_MACOS=${2-}

if [ -n "$CROSS_COMPILE_MACOS" -a $(uname -s) != "Linux" ]; then
  echo "Can only cross compile to macOS from 'Linux'; 'uname -s' is $(uname -s)"
  exit 1
fi

if [ -n "$CROSS_COMPILE_MACOS" ]; then
  SQLCIPHER_DIR=$(abspath "desktop/darwin/sqlcipher")
  OPENSSL_DIR=$(abspath "desktop/darwin/openssl")
elif [ $(uname -s) == "Darwin" ]; then
  SQLCIPHER_DIR=$(abspath "desktop/darwin/sqlcipher")
  OPENSSL_DIR=$(abspath "desktop/darwin/openssl")
elif [ $(uname -s) == "Linux" ]; then
  # This is a JNA weirdness: "x86-64" rather than "x86_64".
  SQLCIPHER_DIR=$(abspath "desktop/linux-x86-64/sqlcipher")
  OPENSSL_DIR=$(abspath "desktop/linux-x86-64/openssl")
else
   echo "Cannot build SQLcipher on unrecognized host OS $(uname -s)"
   exit 1
fi

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

make clean || true

# Why `--with-pic --enable-shared`?  We're doing unusual things.  By
# default, libtool builds a static library (.a) with a non-PIC .o, and
# a shared library (.so, say) with a PIC .o.  We want to compile PIC
# .o files for use in subsequent compilations and wrap them in a .a.
# We achieve that by forcing PIC (even for the .a) and disabling the
# shared library (.so) entirely.

if [ -n "$CROSS_COMPILE_MACOS" ]; then
  export CC=/tmp/clang/bin/clang

  export AR=/tmp/cctools/bin/x86_64-apple-darwin11-ar
  export RANLIB=/tmp/cctools/bin/x86_64-apple-darwin11-ranlib
  export STRIP=/tmp/cctools/bin/x86_64-apple-darwin11-strip
  export LIBTOOL=/tmp/cctools/bin/x86_64-apple-darwin11-libtool
  export NM=/tmp/cctools/bin/x86_64-apple-darwin11-nm
  export LD=/tmp/cctools/bin/x86_64-apple-darwin11-ld

  export CFLAGS='-B /tmp/cctools/bin -target x86_64-apple-darwin11 -mlinker-version=137 -isysroot /tmp/MacOSX10.11.sdk -I/tmp/MacOSX10.11.sdk/usr/include -iframework /tmp/MacOSX10.11.sdk/System/Library/Frameworks'
  export LDFLAGS='-B /tmp/cctools/bin -Wl,-syslibroot,/tmp/MacOSX10.11.sdk -Wl,-dead_strip'
  # This is crucial.  Without this, libtool drops the `-target ...`
  # flags from the clang compiler linker driver invocation, resulting
  # in clang choosing a random system `ld` rather than the macOS
  # linker from the cctools port.
  export LTLINK_EXTRAS='-XCClinker -target -XCClinker x86_64-apple-darwin11 -XCClinker -B -XCClinker /tmp/cctools/bin'

  # See https://searchfox.org/mozilla-central/rev/8848b9741fc4ee4e9bc3ae83ea0fc048da39979f/build/macosx/cross-mozconfig.common#12-13.
  export LD_LIBRARY_PATH=/tmp/clang/lib

  ../configure --prefix="$PWD/install-prefix" \
    --with-pic \
    --disable-shared \
    --host=x86_64-apple-darwin \
    --with-crypto-lib=none \
    --disable-tcl \
    --enable-tempstore=yes \
    CFLAGS="${CFLAGS} ${SQLCIPHER_CFLAGS} -I${OPENSSL_DIR}/include -L${OPENSSL_DIR}/lib" \
    LDFLAGS="${LDFLAGS} -L${OPENSSL_DIR}/lib" \
    LIBS="-lcrypto"
elif [ $(uname -s) == "Darwin" ]; then
  ../configure --prefix="$PWD/install-prefix" \
    --with-pic \
    --disable-shared \
    --enable-tempstore=yes \
    --with-crypto-lib=none \
    --disable-tcl \
    CFLAGS="${SQLCIPHER_CFLAGS} -I${OPENSSL_DIR}/include -L${OPENSSL_DIR}/lib" \
    LDFLAGS="-L${OPENSSL_DIR}/lib" \
    LIBS="-lcrypto"
elif [ $(uname -s) == "Linux" ]; then
  ../configure --prefix="$PWD/install-prefix" \
    --with-pic \
    --disable-shared \
    --enable-tempstore=yes \
    --with-crypto-lib=none \
    --disable-tcl \
    CFLAGS="${SQLCIPHER_CFLAGS} -I${OPENSSL_DIR}/include -L${OPENSSL_DIR}/lib" \
    LDFLAGS="-L${OPENSSL_DIR}/lib" \
    LIBS="-lcrypto -ldl -lm"
else
   echo "Cannot build SQLcipher on unrecognized host OS $(uname -s)"
   exit 1
fi

make -j6
make install

mkdir -p "$SQLCIPHER_DIR/lib"
cp -r "install-prefix/include" "$SQLCIPHER_DIR"
cp -p "install-prefix/lib/libsqlcipher.a" "$SQLCIPHER_DIR/lib/libsqlcipher.a"

chmod +w "$SQLCIPHER_DIR/lib/libsqlcipher.a"

popd
