#!/usr/bin/env bash

set -euvx

# End of configuration.

if [ "$#" -lt 1 -o "$#" -gt 2 ]
then
  echo "Usage:"
  echo "./build-openssl-desktop.sh <OPENSSL_SRC_PATH> [CROSS_COMPILE_TARGET]"
  exit 1
fi

OPENSSL_SRC_PATH=$1
CROSS_COMPILE_TARGET=${2-}

if [ -n "$CROSS_COMPILE_TARGET" -a $(uname -s) != "Linux" ]; then
  echo "Can only cross compile from 'Linux'; 'uname -s' is $(uname -s)"
  exit 1
fi

if [[ "$CROSS_COMPILE_TARGET" =~ "win32-x86-64" ]]; then
  OPENSSL_DIR=$(abspath "desktop/win32-x86-64/openssl")
elif [[ "$CROSS_COMPILE_TARGET" =~ "darwin" ]]; then
  OPENSSL_DIR=$(abspath "desktop/darwin/openssl")
elif [ -n "$CROSS_COMPILE_TARGET" ]; then
  echo "Cannot build OpenSSL for unrecognized target OS $CROSS_COMPILE_TARGET"
  exit 1
elif [ $(uname -s) == "Darwin" ]; then
  OPENSSL_DIR=$(abspath "desktop/darwin/openssl")
elif [ $(uname -s) == "Linux" ]; then
  # This is a JNA weirdness: "x86-64" rather than "x86_64".
  OPENSSL_DIR=$(abspath "desktop/linux-x86-64/openssl")
else
   echo "Cannot build OpenSSL on unrecognized host OS $(uname -s)"
   exit 1
fi

if [ -d "$OPENSSL_DIR" ]; then
  echo "$OPENSSL_DIR folder already exists. Skipping build."
  exit 0
fi

echo "# Building openssl"
OPENSSL_OUTPUT_PATH="/tmp/openssl"_$$
pushd "${OPENSSL_SRC_PATH}"
mkdir -p "$OPENSSL_OUTPUT_PATH"

if [[ "$CROSS_COMPILE_TARGET" =~ "darwin" ]]; then
  # OpenSSL's configure script isn't very robust: it appears to look
  # in $PATH.  This is all cribbed from
  # https://searchfox.org/mozilla-central/rev/8848b9741fc4ee4e9bc3ae83ea0fc048da39979f/build/macosx/cross-mozconfig.common.
  export PATH=/tmp/clang/bin:/tmp/cctools/bin:$PATH

  export CC=/tmp/clang/bin/clang

  export TOOLCHAIN_PREFIX=/tmp/cctools/bin
  export AR=/tmp/cctools/bin/x86_64-apple-darwin11-ar
  export RANLIB=/tmp/cctools/bin/x86_64-apple-darwin11-ranlib

  LD_LIBRARY_PATH=/tmp/clang/lib ./Configure darwin64-x86_64-cc \
    no-asm shared \
    -march=x86-64 \
    '-B /tmp/cctools/bin' \
    '-target x86_64-apple-darwin11' \
    '-isysroot /tmp/MacOSX10.11.sdk' \
    '-Wl,-syslibroot,/tmp/MacOSX10.11.sdk' \
    '-Wl,-dead_strip' \
    --prefix="$OPENSSL_OUTPUT_PATH"

  sed -i.orig 's/-arch x86_64//' Makefile

  # See https://searchfox.org/mozilla-central/rev/8848b9741fc4ee4e9bc3ae83ea0fc048da39979f/build/macosx/cross-mozconfig.common#12-13.
  export LD_LIBRARY_PATH=/tmp/clang/lib
elif [[ "$CROSS_COMPILE_TARGET" =~ "win32-x86-64" ]]; then
    # Force 64 bits on Windows..
    ./Configure --cross-compile-prefix=x86_64-w64-mingw32- mingw64 \
      shared \
      --prefix="$OPENSSL_OUTPUT_PATH"
elif [ $(uname -s) == "Darwin" ]; then
    # Force 64 bits on macOS.
    ./Configure darwin64-x86_64-cc \
      shared \
      --prefix="$OPENSSL_OUTPUT_PATH"
elif [ $(uname -s) == "Linux" ]; then
    ./config shared \
      --prefix="$OPENSSL_OUTPUT_PATH"
fi

make clean || true
make -j6
make install_sw

mkdir -p "$OPENSSL_DIR""/include/openssl"
mkdir -p "$OPENSSL_DIR""/lib"
cp -p "$OPENSSL_OUTPUT_PATH"/lib/libssl.a "$OPENSSL_DIR""/lib"
cp -p "$OPENSSL_OUTPUT_PATH"/lib/libcrypto.a "$OPENSSL_DIR""/lib"
cp -L "$PWD"/include/openssl/*.h "${OPENSSL_DIR}/include/openssl"
rm -rf "$OPENSSL_OUTPUT_PATH"

if [[ "$CROSS_COMPILE_TARGET" =~ "win32-x86-64" ]]; then
  # See https://www.wagner.pp.ru/~vitus/articles/openssl-mingw.html.
  mv "$OPENSSL_DIR/lib/libssl.a" "$OPENSSL_DIR/lib/libssl.lib"
  mv "$OPENSSL_DIR/lib/libcrypto.a" "$OPENSSL_DIR/lib/libcrypto.lib"
fi

popd
