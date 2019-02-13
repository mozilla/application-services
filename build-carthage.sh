#!/usr/bin/env bash

FRAMEWORK_NAME="${1:-MozillaAppServices-frameworks.zip}"
carthage update --platform iOS swift-protobuf
## When https://github.com/Carthage/Carthage/issues/2623 is fixed,
## carthage build --archive should work to produce a zip
carthage build --no-skip-current --platform iOS --verbose && \
  (cd Carthage/Build/iOS | egrep -v 'Static|Logins|FxAClient' | xargs rm -rf) && \
  zip -r $FRAMEWORK_NAME Carthage/Build/iOS
