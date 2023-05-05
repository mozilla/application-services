#!/bin/bash

set -e

cargo uniffi-bindgen generate components/nimbus/src/cirrus.udl --language python -o .
cargo uniffi-bindgen generate components/support/nimbus-fml/src/fml.udl --language python -o .
cargo build --manifest-path megazords/cirrus/Cargo.toml --release

if [[ "$OSTYPE" == "darwin"* ]]; then
  mv target/release/libcirrus.dylib ./
else
  mv target/release/libcirrus.so ./
fi

PYTHONPATH=$PYTHONPATH:$(pwd) pytest -s automation/python-tests