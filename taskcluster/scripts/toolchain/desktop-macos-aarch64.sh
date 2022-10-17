#!/bin/bash
set -ex
cd vcs
git submodule update --init
./taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh
. ./taskcluster/scripts/toolchain/cross-compile-setup.sh
pushd libs
./build-all.sh darwin-aarch64
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/macos-aarch64.tar.gz libs/desktop
