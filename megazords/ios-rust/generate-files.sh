#!/usr/bin/env bash

set -ex

if [[ $# -ne 2 ]] ; then
    echo "USAGE megazords/ios-rust/generate-files.sh [UNIFFI_BINDGEN_LIBRARY] [COMMON]"
    exit 1
fi

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

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

## Tests will need the generated swift files from uniffi
# TODO: Should we wrap this around an argument? we'd only need this for tests
GENERATED_SWIFT_OUT_DIR="$THIS_DIR/Sources/MozillaRustComponentsWrapper/Generated"
mkdir -p "$GENERATED_SWIFT_OUT_DIR"
"$CARGO" uniffi-bindgen-library-mode -l "$UNIFFI_BINDGEN_LIBRARY" swift --swift-sources "$GENERATED_SWIFT_OUT_DIR"

# Hack to copy in the RustViaductFFI.h (https://bugzilla.mozilla.org/show_bug.cgi?id=1925601)
cp "$THIS_DIR/../../components/viaduct/ios/RustViaductFFI.h" "$COMMON/Headers"
echo "original modulemap"
cat "$COMMON/Modules/module.modulemap"
TWEAKED_MODULEMAP=$(cat <(head -n1 "$COMMON/Modules/module.modulemap") <(echo "    header \"RustViaductFFI.h\"") <(tail -n +2 "$COMMON/Modules/module.modulemap"))
echo "$TWEAKED_MODULEMAP" > "$COMMON/Modules/module.modulemap"
