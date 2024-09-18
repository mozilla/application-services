#!/usr/bin/env bash

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$( dirname "$( dirname "$THIS_DIR" )" )"
WORKING_DIR=$THIS_DIR
echo "Project root directory: $REPO_ROOT"

# Set the documentation directory
KOTLIN_DIR="$WORKING_DIR/src/main/kotlin"

# Generate Kotlin bindings using uniffi-bindgen
CARGO=${CARGO:-cargo}  # Use the provided CARGO or fallback to cargo
$CARGO uniffi-bindgen-library-mode android kotlin "$KOTLIN_DIR"

# Generate documentation with increased memory
(cd "$WORKING_DIR" && "$REPO_ROOT/gradlew" --max-workers=2 dokkaHtml -Dorg.gradle.vfs.watch=false)
echo "Documentation generated in $DOC_DIR/build/dokka directory"
