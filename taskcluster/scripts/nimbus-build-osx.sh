#!/bin/bash

set -ex

# This runs in front of `build-nimbus-fml.py`  The only reason it exists is that it's easier to
# setup the environment in a script.

# Build against whichever macOS SDK the worker's Xcode ships, but pin the deployment target:
# deriving it from the SDK would raise the minimum macOS of the binaries with every Xcode bump.
xcodebuild -showsdks
SDKROOT=$(xcrun -sdk macosx --show-sdk-path)
MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.3}
export SDKROOT
export MACOSX_DEPLOYMENT_TARGET
taskcluster/scripts/nimbus-build.py "$@"
