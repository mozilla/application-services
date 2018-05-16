#!/bin/bash -ex

export JANSSON_DIR="$PWD""/libs/ios/jansson/lib"
export OPENSSL_STATIC=0 OPENSSL_DIR="$PWD""/libs/ios/openssl"
export CJOSE_DIR="$PWD""/libs/ios/cjose/lib"

# Build dependencies
pushd .
cd libs/ios
./build-all.sh
popd

cargo test
