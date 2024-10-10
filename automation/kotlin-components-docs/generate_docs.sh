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

# Generate Kotlin bindings to use as the documentation source
echo "Generating Kotlin bindings with uniffi-bindgen..."

$CARGO uniffi-bindgen-library-mode -m megazord kotlin "$KOTLIN_DIR"

# Generate documentation with increased memory
(cd "$WORKING_DIR" && "$REPO_ROOT/gradlew" --max-workers=2 dokkaHtml -Dorg.gradle.vfs.watch=false)
echo "Documentation successfully generated"
