#!/bin/bash

# Setup the ios libraries built by the `libs-ios.sh`` script.  See that script for details on this process.
#
# This is intended to be sourced in the `pre-commands`
# shellcheck shell=bash

rsync -av "${MOZ_FETCHES_DIR}"/libs/ios/ libs/ios/
