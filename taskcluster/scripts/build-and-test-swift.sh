#!/bin/bash

set -ex

# This runs in front of `build-and-test-swift.py`  The only reason it exists is that it's easier to
# setup the enviroment in a script.

# shellcheck source=/dev/null
source "$HOME/.cargo/env"
export PATH="$HOME/bin:$HOME/Library/Python/3.7/bin:$PATH"
taskcluster/scripts/build-and-test-swift.py build/swift-components build/ build/glean-workdir
