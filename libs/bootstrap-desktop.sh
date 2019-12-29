#!/usr/bin/env bash

# shellcheck disable=SC2148,SC2164
# Set environment variables for using vendored dependencies in desktop builds.
#
# This file should be used via `source ./libs/bootstrap-desktop.sh` and will
# not have the desired effect if you try to run it directly, because it
# uses `export` to set environment variables.

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: bootstrap-desktop.sh should be run from the root directory of the repo"
  exit 1
fi

"$(pwd)/libs/bootstrap-common.sh"

if [[ "$(uname -s)" == "Darwin" ]]; then
  APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/darwin"
else
  APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/linux-x86-64"
fi
export SQLCIPHER_LIB_DIR="${APPSERVICES_PLATFORM_DIR}/sqlcipher/lib"
export SQLCIPHER_INCLUDE_DIR="${APPSERVICES_PLATFORM_DIR}/sqlcipher/include"
export NSS_STATIC="1"
export NSS_DIR="${APPSERVICES_PLATFORM_DIR}/nss"
if [[ ! -d "${SQLCIPHER_LIB_DIR}" ]] || [[ ! -d "${NSS_DIR}" ]]; then
  pushd libs
  ./build-all.sh desktop
  popd
fi;
