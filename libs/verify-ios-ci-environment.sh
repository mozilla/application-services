#!/usr/bin/env bash

# Ensure the build toolchains are set up correctly for iOS builds.
#
# This is intended for use in CI, so it verifies only the minimum that is needed
# to build in CI. For local development use `verify-ios-environment.sh`.
#
# This file should be used via `./libs/verify-ios-ci-environment.sh`.

set -e

RUST_TARGETS=("aarch64-apple-ios" "x86_64-apple-ios")

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: verify-ios-ci-environment.sh should be run from the root directory of the repo"
  exit 1
fi

"$(pwd)/libs/verify-common.sh"

rustup target add "${RUST_TARGETS[@]}"

# If you add a dependency below, mention it in building.md in the iOS section!

if ! [[ -x "$(command -v carthage)" ]]; then
  echo 'Error: Carthage needs to be installed. See https://github.com/Carthage/Carthage#installing-carthage for install instructions.' >&2
  exit 1
fi

if ! [[ -x "$(command -v protoc-gen-swift)" ]]; then
  echo 'Error: swift-protobuf needs to be installed. See https://github.com/apple/swift-protobuf#alternatively-install-via-homebrew for install instructions.' >&2
  exit 1
fi

if ! [[ -x "$(command -v xcpretty)" ]]; then
  echo 'Error: xcpretty needs to be installed. See https://github.com/xcpretty/xcpretty#installation for install instructions.' >&2
  exit 1
fi

# For Xcode 12 make sure EXCLUDED_ARCHS is set to arm architectures otherwise
# the build will fail on lipo due to duplicate architectures.
XCODE_XCCONFIG_FILE=$(pwd)/xcconfig/xcode-12-fix-carthage-lipo.xcconfig
export XCODE_XCCONFIG_FILE

echo "Running carthage boostrap..."
carthage bootstrap --platform iOS --cache-builds

if [[ ! -d "${PWD}/libs/ios/universal/nss" ]] || [[ ! -d "${PWD}/libs/ios/universal/sqlcipher" ]]; then
  pushd libs || exit 1
  ./build-all.sh ios
  popd || exit 1
fi
