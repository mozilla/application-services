#!/usr/bin/env bash

set -euvx

carthage bootstrap --platform iOS --cache-builds

set -o pipefail && \
xcodebuild \
  -workspace ./megazords/ios/MozillaAppServices.xcodeproj/project.xcworkspace \
  -scheme MozillaAppServices \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPhone 8' \
  test | \
tee raw_xcodetest.log | \
xcpretty && exit "${PIPESTATUS[0]}"
