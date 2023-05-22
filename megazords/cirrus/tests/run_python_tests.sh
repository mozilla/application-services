#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

set -e

cargo uniffi-bindgen generate components/nimbus/src/cirrus.udl --language python -o .
cargo uniffi-bindgen generate components/support/nimbus-fml/src/fml.udl --language python -o .
cargo build --manifest-path megazords/cirrus/Cargo.toml --release

if [[ "$OSTYPE" == "darwin"* ]]; then
  mv target/release/libcirrus.dylib ./
else
  mv target/release/libcirrus.so ./
fi

PYTHONPATH=$PYTHONPATH:$(pwd) pytest -s megazords/cirrus/tests/python-tests