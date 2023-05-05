#!/bin/bash

set -ex

# This runs in front of `build-nimbus-fml.py`  The only reason it exists is that it's easier to
# setup the enviroment in a script.

# shellcheck source=/dev/null
source "$HOME/.cargo/env"
export PATH="$HOME/bin:$HOME/Library/Python/3.7/bin:$PATH"
SDK=macosx11.0
xcodebuild -showsdks
SDKROOT=$(xcrun -sdk $SDK --show-sdk-path)
MACOSX_DEPLOYMENT_TARGET=$(xcrun -sdk $SDK --show-sdk-platform-version)
export SDKROOT
export MACOSX_DEPLOYMENT_TARGET
taskcluster/scripts/nimbus-build.py "$@"
