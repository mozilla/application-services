#!/usr/bin/env bash
#
# This script builds the Rust crate in its directory and generates Swift bindings, headers, and a module map.

FRAMEWORK_NAME="SwiftComponents"

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$( dirname "$( dirname "$THIS_DIR" )" )"
WORKING_DIR=$THIS_DIR

MANIFEST_PATH="$WORKING_DIR/Cargo.toml"

if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "Could not locate Cargo.toml in $MANIFEST_PATH"
  exit 1
fi

CRATE_NAME=$(grep --max-count=1 '^name =' "$MANIFEST_PATH" | cut -d '"' -f 2)
if [[ -z "$CRATE_NAME" ]]; then
  echo "Could not determine crate name from $MANIFEST_PATH"
  exit 1
fi

# Helper to run the cargo build command in a controlled environment.
# It's important that we don't let environment variables from the user's default
# desktop build environment leak into the iOS build, otherwise it might e.g.
# link against the desktop build of NSS.

CARGO="$HOME/.cargo/bin/cargo"
LIBS_DIR="$REPO_ROOT/libs"

cargo_build () {
  LIBS_DIR="$REPO_ROOT/libs/ios/arm64"

  env -i \
    NSS_STATIC=1 \
    NSS_DIR="$LIBS_DIR/nss" \
    PATH="${PATH}"
}

set -euvx

cargo_build aarch64-apple-ios

# Create directories for Swift files, headers, and module map
INCLUDE_DIR="$WORKING_DIR/Sources/$FRAMEWORK_NAME/include"
SWIFT_DIR="$WORKING_DIR/Sources/$FRAMEWORK_NAME"

mkdir -p "$INCLUDE_DIR"

# Generate Swift bindings and headers using uniffi-bindgen
$CARGO uniffi-bindgen-library-mode ios swift "$SWIFT_DIR"

# Move header files to the include directory
mv "$SWIFT_DIR"/*.h "$INCLUDE_DIR"

rm -rf "$WORKING_DIR"/Sources/"$FRAMEWORK_NAME"/*.modulemap

echo "Successfully generated Swift bindings, headers, and module map."
