#!/usr/bin/env bash
#
# This script builds the Rust crate in its directory and generates Swift bindings,
# headers, and a module map using UniFFI in library mode.

set -euo pipefail  # Ensure script exits on errors or unset variables

FRAMEWORK_NAME="SwiftComponents"
THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$( dirname "$( dirname "$THIS_DIR" )" )"
WORKING_DIR="$THIS_DIR"
CARGO="$HOME/.cargo/bin/cargo"

# Create directories for Swift files, headers, and module map
INCLUDE_DIR="$WORKING_DIR/Sources/$FRAMEWORK_NAME/include"
SWIFT_DIR="$WORKING_DIR/Sources/$FRAMEWORK_NAME"
mkdir -p "$INCLUDE_DIR"

# Build the Rust crate using Cargo
echo "Building the Rust crate..."
$CARGO build -p megazord --release

# Define the path to the generated Rust library
LIBRARY_FILE="$REPO_ROOT/target/release/libmegazord.dylib"
if [[ ! -f "$LIBRARY_FILE" ]]; then
  echo "Error: Rust library not found at $LIBRARY_FILE"
  exit 1
fi

# Generate Swift bindings, headers, and module map using uniffi-bindgen
echo "Generating Swift bindings with uniffi-bindgen..."
$CARGO uniffi-bindgen generate --library "$LIBRARY_FILE" --language swift --out-dir "$SWIFT_DIR"

# Move generated header files to the include directory
echo "Moving header files to include directory..."
mv "$SWIFT_DIR"/*.h "$INCLUDE_DIR" || {
  echo "Error: Failed to move header files."
  exit 1
}

# Remove any old modulemaps
echo "Cleaning up old module maps..."
rm -f "$SWIFT_DIR"/*.modulemap

# Generate a new module map
echo "Generating module map..."
if [[ ! -f "$WORKING_DIR/generate-modulemap.sh" ]]; then
  echo "Error: generate-modulemap.sh script not found."
  exit 1
fi
"$WORKING_DIR/generate-modulemap.sh"

# Success message
echo "Successfully generated Swift bindings, headers, and module map."
