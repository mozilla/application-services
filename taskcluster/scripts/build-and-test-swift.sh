#!/bin/bash

set -ex

# This runs in front of `build-and-test-swift.py`  The only reason it exists is that it's easier to
# setup the environment in a script.

taskcluster/scripts/build-and-test-swift.py build/swift-components artifacts/ build/glean-workdir
