#!/usr/bin/env bash

# Ensure the build toolchains are set up correctly for iOS builds.
#
# This file should be used via `./libs/verify-ios-environment.sh`.

set -e

RUST_TARGETS=("aarch64-apple-ios" "x86_64-apple-ios")

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: verify-ios-environment.sh should be run from the root directory of the repo"
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

set -euo pipefail

xcconfig=$(mktemp /tmp/static.xcconfig.XXXXXX)
trap 'rm -f "$xcconfig"' INT TERM HUP EXIT

# For Xcode 12 make sure EXCLUDED_ARCHS is set to arm architectures otherwise
# the build will fail on lipo due to duplicate architectures.
echo 'EXCLUDED_ARCHS__EFFECTIVE_PLATFORM_SUFFIX_simulator__NATIVE_ARCH_64_BIT_x86_64__XCODE_1200 = arm64 arm64e armv7 armv7s armv6 armv8' >> $xcconfig
echo 'EXCLUDED_ARCHS = $(inherited) $(EXCLUDED_ARCHS__EFFECTIVE_PLATFORM_SUFFIX_$(EFFECTIVE_PLATFORM_SUFFIX)__NATIVE_ARCH_64_BIT_$(NATIVE_ARCH_64_BIT)__XCODE_$(XCODE_VERSION_MAJOR))' >> $xcconfig

export XCODE_XCCONFIG_FILE="$xcconfig"

echo "Running carthage boostrap..."
carthage bootstrap --platform iOS --cache-builds

if [[ ! -d "${PWD}/libs/ios/universal/nss" ]] || [[ ! -d "${PWD}/libs/ios/universal/sqlcipher" ]]; then
  pushd libs || exit 1
  ./build-all.sh ios
  popd || exit 1
fi

echo ""
echo "Looks good! You can do the following:"
echo "- Build the project:"
echo "    ./build-carthage.sh --no-archive"
echo "- Open the XCode project:"
echo "    open megazords/ios/MozillaAppServices.xcodeproj"
echo "- Run the iOS tests:"
echo "    ./automation/run_ios_tests.sh"
