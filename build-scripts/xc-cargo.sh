#!/usr/bin/env bash
#
# This is a small wrapper for running `cargo` inside of an XCode build,
# which unfortunately doesn't seem to work quite right out-of-the-box.
set -eEuvx


# XCode tries to be helpful and overwrites the PATH. Reset that.
PATH="$(bash -l -c 'echo $PATH')"

"${HOME}"/.cargo/bin/cargo "${@:-help}"
