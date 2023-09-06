#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

set -e

if [[ "$OSTYPE" == "darwin"* ]]; then
  LIBCIRRUS_PATH=target/release/libcirrus.dylib
else
  LIBCIRRUS_PATH=target/release/libcirrus.so
fi

cargo build --manifest-path megazords/cirrus/Cargo.toml --release

cargo uniffi-bindgen generate --library $LIBCIRRUS_PATH --language python -o .

cp $LIBCIRRUS_PATH ./

PYTHONPATH=$PYTHONPATH:$(pwd) pytest -s megazords/cirrus/tests/python-tests
