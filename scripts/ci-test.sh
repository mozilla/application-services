#!/bin/bash -ex

files=(
    "fxa-rust-client"
    "fxa-rust-client/ffi"
    "rust-cjose"
    "boxlocker"
    "sync15-adapter"
    "sync15-adapter/ffi"
)



for i in "${files[@]}"
do
   :
   echo "Testing: $i"
   cd $i
   cargo build --release
   cargo test
   cd ..
done
