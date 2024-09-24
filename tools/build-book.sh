#!/bin/bash
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Build all docs with one command
# Documentation will be placed in `build/docs`.

set -xe

if [[ ! -f "$(pwd)/tools/build-book.sh" ]]; then
  echo "ERROR: build-book.sh should be run from the root directory of the repo"
  exit 1
fi

# Generate the Rust docs and move them over to the book.
# We document all features. Ideally we'd leverage https://docs.rs/document-features/latest/document_features/
# so the features themselves are documented, but that's another dependency or another yak, so for another day.
cargo doc --all-features --no-deps
echo '<meta http-equiv=refresh content=0;url=fxa_client>' > target/doc/index.html
mkdir -p docs/rust-docs
cp -rf target/doc/* docs/rust-docs

# Build the development book
output=$(mdbook build docs 2>&1)
if echo "$output" | grep -q "\[ERROR\]" ; then
    exit 1
fi

# copy the output files to the publishing directory
rm -rf build/docs
mkdir -p build/docs
echo '<meta http-equiv=refresh content=0;url=book/index.html>' > build/docs/index.html

mkdir -p build/docs
cp -a docs/book/. build/docs/book

mkdir -p build/docs/shared
cp -a docs/shared/. build/docs/shared