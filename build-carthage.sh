#!/usr/bin/env bash

set -eu

CONFIGURATION="Release"
FRAMEWORK_NAME="MozillaAppServices.framework.zip"

while [[ "$#" -gt 0 ]]; do case $1 in
  --configuration) CONFIGURATION="$2"; shift;shift;;
  --out) FRAMEWORK_NAME="$2"; shift;shift;;
  *) echo "Unknown parameter: $1"; exit 1;
esac; done

set -vx

# Help out iOS folks who might want to run this but haven't
# updated rust recently.
rustup update stable

carthage bootstrap --platform iOS --cache-builds
## When https://github.com/Carthage/Carthage/issues/2623 is fixed,
## carthage build --archive should work to produce a zip

set -o pipefail && \
carthage build --no-skip-current --platform iOS --verbose --configuration "${CONFIGURATION}" --cache-builds | \
tee raw_xcodebuild.log | \
xcpretty

# Exclude SwiftProtobuf.
zip -r "${FRAMEWORK_NAME}" Carthage/Build/iOS megazords/ios/DEPENDENCIES.md -x '*SwiftProtobuf.framework*/*'
