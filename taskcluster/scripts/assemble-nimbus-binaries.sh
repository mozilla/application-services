#!/bin/sh

set -ex
BUILD_DIR=$(realpath build)
mkdir -p "${BUILD_DIR}"

cd "${MOZ_FETCHES_DIR}"/nimbus-fml
zip "${BUILD_DIR}/nimbus-fml.zip" -r .

cd "${MOZ_FETCHES_DIR}"/nimbus-cli
zip "${BUILD_DIR}/nimbus-cli.zip" -r .

cd "${BUILD_DIR}"
sha256sum nimbus-cli.zip > nimbus-cli.sha256
sha256sum nimbus-fml.zip > nimbus-fml.sha256
