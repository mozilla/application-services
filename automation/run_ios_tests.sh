#!/usr/bin/env bash

set -euvx

./megazords/ios-rust/build-xcframework.sh --build-profile release
set -o pipefail && \
xcodebuild \
  -workspace ./megazords/ios-rust/MozillaTestServices/MozillaTestServices.xcodeproj/project.xcworkspace \
  -scheme MozillaTestServices \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPhone 14' \
  test | \
tee raw_xcodetest.log | \
xcpretty && exit "${PIPESTATUS[0]}"
