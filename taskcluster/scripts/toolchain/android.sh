#!/bin/bash
set -ex
cd vcs
git submodule update --init
./taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh
./libs/verify-android-ci-environment.sh
pushd libs
./build-all.sh android
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/android.tar.gz libs/android
