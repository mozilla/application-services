#!/bin/bash
set -ex
cd vcs
git submodule update --init
./taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh
./taskcluster/scripts/toolchain/fetch-macos-sdk.sh
. ./taskcluster/scripts/toolchain/cross-compile-setup.sh
set -e
pushd libs
./build-all.sh darwin
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/macos.tar.gz libs/desktop
