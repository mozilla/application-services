#!/bin/bash

set -ex

# This runs in front of `build-and-test-swift.py`  The only reason it exists is that it's easier to
# setup the enviroment in a script.

mv "$MOZ_FETCHES_DIR/swiftformat" "$HOME/bin/swiftformat"
taskcluster/scripts/build-and-test-swift.py build/swift-components build/ build/glean-workdir
