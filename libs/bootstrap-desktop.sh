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

if [[ "$(uname -s)" == "Darwin" ]]; then
  APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/darwin"
else
  APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/linux-x86-64"
fi
export SQLCIPHER_LIB_DIR="${APPSERVICES_PLATFORM_DIR}/sqlcipher/lib"
export SQLCIPHER_INCLUDE_DIR="${APPSERVICES_PLATFORM_DIR}/sqlcipher/include"
export OPENSSL_DIR="${APPSERVICES_PLATFORM_DIR}/openssl"
export NSS_STATIC="1"
export NSS_DIR="${APPSERVICES_PLATFORM_DIR}/nss"
if [[ ! -d "${SQLCIPHER_LIB_DIR}" ]] || [[ ! -d "${OPENSSL_DIR}" ]] || [[ ! -d "${NSS_DIR}" ]]; then
  pushd libs
  ./build-all.sh desktop
  popd
fi;
if [[ "$(uname -s)" == "Darwin" ]] && [[ ! -f "/usr/include/pthread.h" ]]; then
  # rustc does not include the macOS SDK headers in its include list yet
  # (see https://developer.apple.com/documentation/xcode_release_notes/xcode_10_release_notes)
  echo "macOS system headers are not installed in /usr/include, please run:"
  echo "open /Library/Developer/CommandLineTools/Packages/macOS_SDK_headers_for_macOS_10.14.pkg"
fi
