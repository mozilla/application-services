#!/usr/bin/env bash

# Ensure the build toolchains are set up correctly for iOS builds.
#
# This is intended for use in CI, so it verifies only the minimum that is needed
# to build in CI. For local development use `verify-ios-environment.sh`.
#
# This file should be used via `./libs/verify-ios-ci-environment.sh`.

set -e

RUST_TARGETS=("aarch64-apple-ios" "x86_64-apple-ios" "aarch64-apple-ios-sim")

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: verify-ios-ci-environment.sh should be run from the root directory of the repo"
  exit 1
fi

"$(pwd)/libs/verify-common.sh"

rustup target add "${RUST_TARGETS[@]}"

# If you add a dependency below, mention it in building.md in the iOS section!

if ! [[ -x "$(command -v xcpretty)" ]]; then
  echo 'Error: xcpretty needs to be installed. See https://github.com/xcpretty/xcpretty#installation for install instructions.' >&2
  exit 1
fi

if [[ ! -d "${PWD}/libs/ios/universal/nss" ]]; then
  pushd libs || exit 1
  ./build-all.sh ios
  popd || exit 1
fi
