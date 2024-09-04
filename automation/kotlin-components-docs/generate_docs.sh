#!/usr/bin/env bash

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$( dirname "$( dirname "$THIS_DIR" )" )"
WORKING_DIR=$THIS_DIR
echo "Project root directory: $REPO_ROOT"

# Set the documentation directory
KOTLIN_DIR="$WORKING_DIR/src/main/kotlin"

# Generate Kotlin bindings using uniffi-bindgen
CARGO=${CARGO:-cargo}  # Use the provided CARGO or fallback to cargo

$CARGO uniffi-bindgen generate "$REPO_ROOT/components/remote_settings/src/remote_settings.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/nimbus/src/nimbus.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/support/error/src/errorsupport.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/support/rust-log-forwarder/src/rust_log_forwarder.udl" -l kotlin -o "$KOTLIN_DIR"

# Repeat for other components
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/crashtest/src/crashtest.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/fxa-client/src/fxa_client.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/logins/src/logins.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/autofill/src/autofill.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/push/src/push.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/tabs/src/tabs.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/places/src/places.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/suggest/src/suggest.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/sync_manager/src/syncmanager.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/sync15/src/sync15.udl" -l kotlin -o "$KOTLIN_DIR"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/as-ohttp-client/src/as_ohttp_client.udl" -l kotlin -o "$KOTLIN_DIR"

# Generate documentation with increased memory
(cd "$WORKING_DIR" && "$REPO_ROOT/gradlew" --max-workers=2 dokkaHtml -Dorg.gradle.vfs.watch=false)
echo "Documentation generated in $DOC_DIR/build/dokka directory"
