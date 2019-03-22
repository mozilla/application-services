#!/usr/bin/env bash

set -euvx

FRAMEWORK_NAME="${1:-MozillaAppServices.framework.zip}"
carthage update --platform iOS swift-protobuf
## When https://github.com/Carthage/Carthage/issues/2623 is fixed,
## carthage build --archive should work to produce a zip

carthage build --no-skip-current --platform iOS --verbose

ZIP_DIR=$(mktemp -d)
mkdir -p $ZIP_DIR/Carthage/Build/iOS
cp -r Carthage/Build/iOS/Static $ZIP_DIR/Carthage/Build/iOS
cp -r Carthage/Build/iOS/*.framework $ZIP_DIR/Carthage/Build/iOS
pushd $ZIP_DIR
# Exclude SwiftProtobuf.
rm -rf Carthage/Build/iOS/SwiftProtobuf.framework
zip -r $FRAMEWORK_NAME Carthage/Build/iOS
popd
cp $ZIP_DIR/$FRAMEWORK_NAME $FRAMEWORK_NAME
