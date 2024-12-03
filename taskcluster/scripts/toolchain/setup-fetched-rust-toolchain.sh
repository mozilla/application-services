#!/bin/bash

# Setup the Rust toolchain built by the `build-rust-toolchain-sh` script.  See that script for details on this process.
#
# This is intended to be sourced in the `pre-commands`
# shellcheck shell=bash

# The artifacts from `build-rust-toolchain.sh` have been fetched to MOZ_FETCHES_DIR.  Copy them out
# to our home directory
rsync -a "${MOZ_FETCHES_DIR}"/.cargo "${HOME}"
rsync -a "${MOZ_FETCHES_DIR}"/.rustup "${HOME}"

# shellcheck source=/dev/null
source "$HOME"/.cargo/env
