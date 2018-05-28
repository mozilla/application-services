#!/bin/bash -ex

export JANSSON_DIR="$PWD""/libs/desktop/jansson/lib"
export OPENSSL_STATIC=0 OPENSSL_DIR="$PWD""/libs/desktop/openssl"
export CJOSE_DIR="$PWD""/libs/desktop/cjose/lib"

# Build dependencies
pushd .
cd libs
./build-all.sh desktop
popd

export LD_LIBRARY_PATH="$JANSSON_DIR:$OPENSSL_DIR/lib:$CJOSE_DIR"
cargo test
