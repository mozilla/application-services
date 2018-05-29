#!/bin/bash

set -e

# End of configuration.

if [ "$#" -ne 3 ]
then
    echo "Usage:"
    echo "./build-all-desktop.sh <JANSSON_SRC_PATH> <OPENSSL_SRC_PATH> <CJOSE_SRC_PATH>"
    exit 1
fi

JANSSON_SRC_PATH=$1
OPENSSL_SRC_PATH=$2
CJOSE_SRC_PATH=$3

JANSSON_DIR=$(abspath "desktop/jansson")
OPENSSL_DIR=$(abspath "desktop/openssl")
CJOSE_DIR=$(abspath "desktop/cjose")

if [ $(uname -s) == "Darwin" ]; then
  LIB_EXTENSION=dylib
else
  LIB_EXTENSION=so
fi

if [ -d "$JANSSON_DIR" ]; then
  echo "$JANSSON_DIR"" folder already exists. Skipping build."
else
  echo "# Building jansson"
  cd "${JANSSON_SRC_PATH}"
  rm -rf lib && rm -rf include
  cmake -DJANSSON_BUILD_SHARED_LIBS=1 && make
  mkdir -p "$JANSSON_DIR""/include"
  mkdir -p "$JANSSON_DIR""/lib"
  cp -p lib/libjansson."$LIB_EXTENSION"* "$JANSSON_DIR""/lib"
  cp "$PWD"/include/*.h "$JANSSON_DIR""/include"
  cd ..
fi

if [ -d "$OPENSSL_DIR" ]; then
  echo "$OPENSSL_DIR"" folder already exists. Skipping build."
else
  echo "# Building openssl"
  OPENSSL_OUTPUT_PATH="/tmp/openssl"_$$
  cd "${OPENSSL_SRC_PATH}"
  mkdir -p "$OPENSSL_OUTPUT_PATH"
  make clean || true
  if [ $(uname -s) == "Darwin" ]; then
    ./Configure darwin64-x86_64-cc shared --openssldir="$OPENSSL_OUTPUT_PATH" # Force 64 bits on macOS
  else
    ./config shared --openssldir="$OPENSSL_OUTPUT_PATH"
  fi
  make && make install
  mkdir -p "$OPENSSL_DIR""/include/openssl"
  mkdir -p "$OPENSSL_DIR""/lib"
  cp -p "$OPENSSL_OUTPUT_PATH"/lib/libssl."$LIB_EXTENSION"* "$OPENSSL_DIR""/lib"
  cp -p "$OPENSSL_OUTPUT_PATH"/lib/libcrypto."$LIB_EXTENSION"* "$OPENSSL_DIR""/lib"
  cp -L "$PWD"/include/openssl/*.h "${OPENSSL_DIR}/include/openssl"
  rm -rf "$OPENSSL_OUTPUT_PATH"
  cd ..
fi

if [ -d "$CJOSE_DIR" ]; then
  echo "$CJOSE_DIR"" folder already exists. Skipping build."
else
  echo "# Building cjose"
  cd "${CJOSE_SRC_PATH}"
  make clean || true
  ./configure --with-openssl="$OPENSSL_DIR" --with-jansson="$JANSSON_DIR" && make
  mkdir -p "$CJOSE_DIR""/include"
  mkdir -p "$CJOSE_DIR""/lib"
  cp -p src/.libs/libcjose."$LIB_EXTENSION"* "$CJOSE_DIR""/lib"
  cp "$PWD"/include/cjose/*.h "$CJOSE_DIR""/include"
  cd ..
fi

