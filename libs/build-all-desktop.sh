#!/bin/bash

set -e

# End of configuration.

if [ "$#" -ne 1 ]
then
    echo "Usage:"
    echo "./build-all-desktop.sh <OPENSSL_SRC_PATH>"
    exit 1
fi

OPENSSL_SRC_PATH=$1
OPENSSL_DIR=$(abspath "desktop/openssl")

if [ $(uname -s) == "Darwin" ]; then
  LIB_EXTENSION=dylib
else
  LIB_EXTENSION=so
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
