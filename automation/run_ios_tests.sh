#!/usr/bin/env bash

set -euvx

./megazords/ios-rust/build-xcframework.sh --build-profile release
set -o pipefail && \
xcodebuild \
  -workspace ./megazords/ios/MozillaAppServices.xcodeproj/project.xcworkspace \
  -scheme MozillaAppServices \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPhone 11' \
  test | \
tee raw_xcodetest.log | \
xcpretty && exit "${PIPESTATUS[0]}"
