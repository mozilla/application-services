#!/usr/bin/env bash

# This script builds the NSS3 library (with NSPR) for Desktop.

set -euvx

if [[ "${#}" -lt 1 ]] || [[ "${#}" -gt 2 ]]
then
  echo "Usage:"
  echo "./build-nss-desktop.sh <ABSOLUTE_SRC_DIR> [CROSS_COMPILE_TARGET]"
  exit 1
fi

NSS_SRC_DIR=${1}
# Whether to cross compile from Linux to a different target.  Really
# only intended for automation.
CROSS_COMPILE_TARGET=${2-}

if [[ -n "${CROSS_COMPILE_TARGET}" ]] && [[ "$(uname -s)" != "Linux" ]]; then
  echo "Can only cross compile from 'Linux'; 'uname -s' is $(uname -s)"
  exit 1
fi

if [[ "${CROSS_COMPILE_TARGET}" == "darwin-x86-64" ]]; then
  DIST_DIR=$(abspath "desktop/darwin-x86-64/nss")
  TARGET_OS="macos"
  TARGET_ARCH="x86_64"
elif [[ -n "${CROSS_COMPILE_TARGET}" ]]; then
  echo "Cannot build NSS for unrecognized target OS ${CROSS_COMPILE_TARGET}"
  exit 1
elif [[ "$(uname -s)" == "Darwin" ]]; then
  TARGET_OS="macos"
  # We need to set this variable for switching libs based on different macos archs (M1 vs Intel)
  if [[ "$(uname -m)" == "arm64" ]]; then
    DIST_DIR=$(abspath "desktop/darwin-aarch64/nss")
    TARGET_ARCH="aarch64"
  else
    DIST_DIR=$(abspath "desktop/darwin-x86-64/nss")
    TARGET_ARCH="x86_64"
  fi
elif [[ "$(uname -s)" == "Linux" ]]; then
  # This is a JNA weirdness: "x86-64" rather than "x86_64".
  DIST_DIR=$(abspath "desktop/linux-x86-64/nss")
  TARGET_OS="linux"
  TARGET_ARCH="x86_64"
else
   echo "Cannot build NSS on unrecognized host OS $(uname -s)"
   exit 1
fi

if [[ -d "${DIST_DIR}" ]]; then
  echo "${DIST_DIR} folder already exists. Skipping build."
  exit 0
fi

# We do not know how to cross compile these, so we pull pre-built versions from NSS CI
# https://github.com/mozilla/application-services/issues/962
if [[ "${CROSS_COMPILE_TARGET}" == "darwin-x86-64" ]]; then
  if [[ "${MOZ_AUTOMATION}" == "1" ]]; then
    # run-task has already downloaded + extracted the dependency
    NSS_DIST_DIR="${MOZ_FETCHES_DIR}/dist"
  else
    # From https://firefox-ci-tc.services.mozilla.com/tasks/index/app-services.cache.level-3.content.v1.nss-artifact/latest
    curl -sfSL --retry 5 --retry-delay 10 -O "https://firefox-ci-tc.services.mozilla.com/api/index/v1/task/app-services.cache.level-3.content.v1.nss-artifact.latest/artifacts/public%2Fdist.tar.bz2"
    SHA256="4cf4c0b4a832ef1804adb59c7d4e6023eaf41e1110619e17836721ccde51a5ef"
    echo "${SHA256}  dist.tar.bz2" | shasum -a 256 -c - || exit 2
    tar xvjf dist.tar.bz2 && rm -rf dist.tar.bz2
    NSS_DIST_DIR=$(abspath "dist")
  fi
elif [[ "$(uname -s)" == "Darwin" ]] || [[ "$(uname -s)" == "Linux" ]]; then
  "${NSS_SRC_DIR}"/nss/build.sh \
    -v \
    --opt \
    --static \
    --disable-tests \
    --python=python3 \
    -Ddisable_dbm=1 \
    -Dsign_libs=0 \
    -Ddisable_libpkix=1
  NSS_DIST_DIR="${NSS_SRC_DIR}/dist"
fi

# Assemble the DIST_DIR with relevant libraries and headers
./copy-nss-libs.sh \
  "${TARGET_OS}" \
  "${TARGET_ARCH}" \
  "${DIST_DIR}" \
  "${NSS_DIST_DIR}/Release/lib" \
  "${NSS_DIST_DIR}/Release/lib" \
  "${NSS_DIST_DIR}/public/nss" \
  "${NSS_DIST_DIR}/Release/include/nspr"

rm -rf "${NSS_DIST_DIR}"
