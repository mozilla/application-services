#!/usr/bin/env bash

set -euvx

pushd libs/ && ./build-all.sh android && popd
