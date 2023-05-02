#!/bin/bash

set -e

cd components/nimbus
cargo build --no-default-features
cargo uniffi-bindgen generate src/cirrus.udl --language python -o .
cargo build --manifest-path ../../megazords/cirrus/Cargo.toml --release

if [[ "$OSTYPE" == "darwin"* ]]; then
  mv ../../target/release/libcirrus.dylib ./libuniffi_cirrus.dylib
else
  mv ../../target/release/libcirrus.so ./libuniffi_cirrus.so
fi

cd ../..
PYTHONPATH=$PYTHONPATH:$(pwd) pytest -s automation/python-tests