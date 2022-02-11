#!/bin/bash
set -e
cd src
./libs/verify-android-ci-environment.sh
pushd libs
./build-all.sh android
popd
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/android.tar.gz libs/android
