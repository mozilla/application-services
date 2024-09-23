#!/usr/bin/env bash

# This script builds the Rust crate in its directory and generates Kotlin bindings,
# using UniFFI in library mode.

# Ensure script exits on errors or unset variables
set -euo pipefail

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$( dirname "$( dirname "$THIS_DIR" )" )"
WORKING_DIR=$THIS_DIR
CARGO="$HOME/.cargo/bin/cargo"

# Set the documentation directory
KOTLIN_DIR="$WORKING_DIR/src/main/kotlin"

# Build the Rust crate using Cargo
echo "Building the Rust crate..."
$CARGO build -p megazord --release

# Define the path to the generated Rust library
LIBRARY_FILE="$REPO_ROOT/target/release/libmegazord.so"
if [[ ! -f "$LIBRARY_FILE" ]]; then
  echo "Error: Rust library not found at $LIBRARY_FILE"
  exit 1
fi

# Generate Kotlin bindings, headers, and module map using uniffi-bindgen
echo "Generating Kotlin bindings with uniffi-bindgen..."
$CARGO uniffi-bindgen generate --library "$LIBRARY_FILE" --language kotlin --out-dir "$KOTLIN_DIR"

# Generate documentation with increased memory
(cd "$WORKING_DIR" && "$REPO_ROOT/gradlew" --max-workers=2 dokkaHtml -Dorg.gradle.vfs.watch=false)
echo "Documentation successfully generated"
