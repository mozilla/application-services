#!/usr/bin/env bash

# Ensure the build toolchains are set up correctly for iOS builds.
#
# This file should be used via `./libs/verify-ios-environment.sh`.

set -e

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: verify-ios-environment.sh should be run from the root directory of the repo"
  exit 1
fi

# iOS consumers are likely to also want to be able to run a quick
# `cargo build` for their desktop env, so verify that as well.
"$(pwd)/libs/verify-desktop-environment.sh"

"$(pwd)/libs/verify-ios-ci-environment.sh"

echo ""
echo "Looks good! Next steps:"
echo "- Build the XCFramework:"
echo "    ./megazords/ios-rust/build-xcframework.sh"
echo ""
echo " Then you'll be able do one of the following: "
echo "- Run the tests via the XCode project:"
echo "    open megazords/ios-rust/MozillaTestServices.xcodeproj"
echo ""
echo "- Run the iOS tests via command line:"
echo "    ./automation/run_ios_tests.sh"
