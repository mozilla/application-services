#!/usr/bin/env bash

set -ex

if [[ $# -ne 2 ]] ; then
    echo "USAGE megazords/ios-rust/generate-files.sh [UNIFFI_BINDGEN_LIBRARY] [COMMON]"
    exit 1
fi

UNIFFI_BINDGEN_LIBRARY=$1
COMMON=$2

# Helper to run the cargo build command in a controlled environment.
# It's important that we don't let environment variables from the user's default
# desktop build environment leak into the iOS build, otherwise it might e.g.
# link against the desktop build of NSS.
CARGO="$HOME/.cargo/bin/cargo"

# Run uniffi-bindgen-library-mode to generate the files.
#
# We can't use the `-m` flag because UNIFFI_BINDGEN_LIBRARY is cross-compiled, which our
# uniffi-bindgen-library-mode tool can't handle yet.
"$CARGO" uniffi-bindgen-library-mode -l "$UNIFFI_BINDGEN_LIBRARY" swift --headers "$COMMON/Headers"
"$CARGO" uniffi-bindgen-library-mode -l "$UNIFFI_BINDGEN_LIBRARY" swift --modulemap "$COMMON/Modules" --xcframework --modulemap-filename module.modulemap
