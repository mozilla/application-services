#!/bin/bash
set -e
cd src
. ./taskcluster/scripts/toolchain/cross-compile-setup.sh
pushd libs
./build-all.sh darwin
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/macos.tar.gz libs/desktop
