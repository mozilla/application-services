#!/bin/bash
set -e
cd src
. ./taskcluster/scripts/toolchain/rustup-setup.sh
pushd libs
./build-all.sh win32-x86-64
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/win.tar.gz libs/desktop
