# Script to setup the mac workers
#
# This is intended to be sourced in `pre-commands`
#
# shellcheck shell=bash

mkdir -p "$HOME/bin"
export PATH="$HOME/bin:$HOME/Library/Python/3.7/bin:$PATH"

# UPLOAD_DIR is not set for the generic worker, so we need to set it ourselves
# FIXME: what's the right way to get this value?
export UPLOAD_DIR="${PWD}/../public/build"
