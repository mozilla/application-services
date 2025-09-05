#!/usr/bin/env bash

set -eu


SOURCE_ROOT=$(pwd)
export SOURCE_ROOT
export PROJECT=MozillaRustComponentsWrapper

# Glean deletes everything in the folder it outputs, so we keep them in their own dir
./components/external/glean/glean-core/ios/sdk_generator.sh \
    -g Glean \
    -o ./megazords/ios-rust/Sources/MozillaRustComponentsWrapper/Generated/Glean \
  "${SOURCE_ROOT}"/components/nimbus/metrics.yaml \
  "${SOURCE_ROOT}"/components/logins/metrics.yaml \
  "${SOURCE_ROOT}"/components/sync_manager/metrics.yaml \
  "${SOURCE_ROOT}"/components/sync_manager/pings.yaml

# Build the XCFramework
./megazords/ios-rust/build-xcframework.sh --generate-swift-sources --build-profile release
