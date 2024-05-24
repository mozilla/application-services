#!/bin/bash

set -ex

# This runs in front of `build-nimbus-fml.py`  The only reason it exists is that it's easier to
# setup the environment in a script.

SDK=macosx13.3
xcodebuild -showsdks
SDKROOT=$(xcrun -sdk $SDK --show-sdk-path)
MACOSX_DEPLOYMENT_TARGET=$(xcrun -sdk $SDK --show-sdk-platform-version)
export SDKROOT
export MACOSX_DEPLOYMENT_TARGET
taskcluster/scripts/nimbus-build.py "$@"
