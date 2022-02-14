#!/bin/bash
set -e
cd src
. ./taskcluster/scripts/toolchain/rustup-setup.sh
./libs/verify-android-ci-environment.sh
pushd libs
./build-all.sh desktop
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/linux.tar.gz libs/desktop
