#!/bin/bash
#
# Setup the Rust toolchain built by the `build-rust-toolchain-sh` script.  See that script for details on this process.

set -ex

# By the time this script runs, `build-rust-toolchain.sh` has completed and all
# the artifacts it's built have been downloaded and unpacked into the fetches
# directory.  We just need to copy them out.
rsync -a /builds/worker/fetches/.cargo /builds/worker/
rsync -a /builds/worker/fetches/.rustup /builds/worker/
