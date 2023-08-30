#!/bin/bash
set -ex
cd vcs
git submodule update --init
source ./taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh
./libs/verify-android-ci-environment.sh
pushd libs
./build-all.sh desktop
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/linux.tar.gz libs/desktop
