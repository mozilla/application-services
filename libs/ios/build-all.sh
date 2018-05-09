#!/bin/bash

set -e

echo "Building all dependencies"
./build-jansson.sh
./build-openssl.sh
./build-cjose.sh
echo "Done"
