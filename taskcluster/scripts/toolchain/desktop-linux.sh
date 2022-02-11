#!/bin/bash
set -e
cd src
./libs/verify-android-ci-environment.sh
pushd libs
./build-all.sh desktop
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/linux.tar.gz libs/desktop
