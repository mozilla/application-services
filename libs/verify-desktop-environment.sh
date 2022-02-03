#!/usr/bin/env bash

# Ensure the build toolchains are set up correctly for desktop builds.
#
# This file should be used via `./libs/verify-desktop-environment.sh`.

set -e

yellow=`tput setaf 3`
reset=`tput sgr0`

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: bootstrap-desktop.sh should be run from the root directory of the repo"
  exit 1
fi

"$(pwd)/libs/verify-common.sh"

if [[ "$(uname -s)" == "Darwin" ]]; then
  if [[ "$(uname -m)" == "arm64" ]]; then
    APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/darwin-aarch64"
  else
    APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/darwin-x86-64"
  fi
else
  APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/linux-x86-64"
fi

if [[ -z "${SQLCIPHER_LIB_DIR}" ]] || [[ -z "${SQLCIPHER_INCLUDE_DIR}" ]] || [[ -z "${NSS_DIR}" ]] || [[ -z "${NSS_STATIC}" ]]; then
  echo "Some necessary environment variables are not set."
  echo "Please export or add to your shell initialization file (.zshenv, .bashrc etc.) the following:"
  echo ""
  echo "export SQLCIPHER_LIB_DIR=${APPSERVICES_PLATFORM_DIR}/sqlcipher/lib"
  echo "export SQLCIPHER_INCLUDE_DIR=${APPSERVICES_PLATFORM_DIR}/sqlcipher/include"
  echo "export NSS_DIR=${APPSERVICES_PLATFORM_DIR}/nss"
  echo "export NSS_STATIC=1"
  exit 1
fi

# If users previously have built NSS their env vars will still be pointing to darwin
# we need to tell them to update their env with the new arch-specific style
if [[ "${NSS_DIR}" != *"desktop/darwin-"*  ]]; then
  echo ""
  echo "${yellow}Your environment variables are outdated! Please use the updated values below"
  echo "Please export or add to your shell initialization file (.zshenv, .bashrc etc.) the following:${reset}"
  echo ""
  echo "export SQLCIPHER_LIB_DIR=${APPSERVICES_PLATFORM_DIR}/sqlcipher/lib"
  echo "export SQLCIPHER_INCLUDE_DIR=${APPSERVICES_PLATFORM_DIR}/sqlcipher/include"
  echo "export NSS_DIR=${APPSERVICES_PLATFORM_DIR}/nss"
  echo "export NSS_STATIC=1"
  exit 1
fi

if [[ ! -d "${SQLCIPHER_LIB_DIR}" ]] || [[ ! -d "${NSS_DIR}" ]]; then
  pushd libs
  ./build-all.sh desktop
  popd
fi;

echo "Looks good! Try running the test suite with \`cargo test\`"
