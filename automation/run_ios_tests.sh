#!/usr/bin/env bash

set -eu

# XCFramework is a slow process rebuilding all the binaries and zipping it
# so we add an option to skip that if we're just trying to change tests
SKIP_BUILDING=false

# Parse command-line arguments
for arg in "$@"; do
  case $arg in
    --test-only)
      SKIP_BUILDING=true
      shift
      ;;
    *)
      echo "Unknown option: $arg" >&2
      exit 1
      ;;
  esac
done


SOURCE_ROOT=$(pwd)
export SOURCE_ROOT
export PROJECT=MozillaRustComponentsWrapper

# Conditionally generate the UniFFi bindings with rust binaries and bundle it into an XCFramework
if [ "$SKIP_BUILDING" != true ]; then

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
else
  echo "Skipping xcframework & glean metrics generation as --test-only was passed."
fi

# xcodebuild needs to run in the directory we have it since
# we are using SPM instead of an Xcode project
pushd megazords/ios-rust > /dev/null

# Temporarily disable "exit immediately" so we can capture the exit code from the pipeline
set +e
set -o pipefail
xcodebuild \
  -scheme MozillaRustComponents \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,OS=17.2,name=iPhone 15' \
  test | tee raw_xcodetest.log | xcpretty
result=${PIPESTATUS[0]}
set -e

# Return to the original directory
popd > /dev/null

# Provide clear messaging based on test results
if [ "$result" -eq 0 ]; then
  echo "✅ Swift tests pass!"
else
  echo "❌ Swift tests failed!"
fi

exit "${result}"
