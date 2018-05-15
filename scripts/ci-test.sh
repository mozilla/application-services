#!/bin/bash -ex

files=(
    "fxa-rust-client"
    "fxa-rust-client/ffi"
    "boxlocker"
    "sync15-adapter"
    "sync15-adapter/ffi"
)

export JANSSON_DIR="$PWD""/libs/ios/jansson/lib"
export OPENSSL_STATIC=0 OPENSSL_DIR="$PWD""/libs/ios/openssl"
export CJOSE_DIR="$PWD""/libs/ios/cjose/lib"

pushd .
cd libs/ios
./build-all.sh
popd

for i in "${files[@]}"
do
   :
   echo "Testing: $i"
   pushd .
   cd $i
   cargo build --release
   cargo test
   popd
done
