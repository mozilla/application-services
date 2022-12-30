#!/usr/bin/env bash
#
# This script builds the Rust crate in its directory into a staticlib XCFramework for iOS.

BUILD_PROFILE="release"
FRAMEWORK_NAME="MozillaRustComponents"
IS_FOCUS=
# FRAMEWORK_FILENAME exist purely because we would like to ship
# multiple frameworks that have the same swift code
# namely for focus. However, componenets that use
# uniffi, can only declare a single framework name.
#
# So we keep the framework the same, but store them
# under different file names.
FRAMEWORK_FILENAME=$FRAMEWORK_NAME
while [[ "$#" -gt 0 ]]; do case $1 in
  --build-profile) BUILD_PROFILE="$2"; shift;shift;;
  --focus) IS_FOCUS="true"; FRAMEWORK_FILENAME="FocusRustComponents";shift;;
  --framework-name) FRAMEWORK_NAME="$2"; shift;shift;;
  *) echo "Unknown parameter: $1"; exit 1;
esac; done

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
WORKING_DIR=
if [[ -n $IS_FOCUS ]]; then
  WORKING_DIR="$THIS_DIR/focus"
else
  WORKING_DIR=$THIS_DIR
fi
REPO_ROOT="$( dirname "$( dirname "$THIS_DIR" )" )"

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

LIB_NAME="lib${CRATE_NAME}.a"

####
##
## 1) Build the rust code individually for each target architecture.
##
####

# Helper to run the cargo build command in a controlled environment.
# It's important that we don't let environment variables from the user's default
# desktop build environment leak into the iOS build, otherwise it might e.g.
# link against the desktop build of NSS.

CARGO="$HOME/.cargo/bin/cargo"
LIBS_DIR="$REPO_ROOT/libs"

DEFAULT_RUSTFLAGS=""
BUILD_ARGS=(build --manifest-path "$MANIFEST_PATH" --lib)
case $BUILD_PROFILE in
  debug) ;;
  release)
    BUILD_ARGS=("${BUILD_ARGS[@]}" --release)
    # With debuginfo, the zipped artifact quickly baloons to many
    # hundred megabytes in size. Ideally we'd find a way to keep
    # the debug info but in a separate artifact.
    DEFAULT_RUSTFLAGS="-C debuginfo=0"
    ;;
  *) echo "Unknown build profile: $BUILD_PROFILE"; exit 1;
esac

cargo_build () {
  TARGET=$1
  case $TARGET in
    x86_64*)
      LIBS_DIR="$REPO_ROOT/libs/ios/x86_64";;
    aarch64*)
      LIBS_DIR="$REPO_ROOT/libs/ios/arm64";;
    *)
      echo "Unexpected target architecture: $TARGET" && exit 1;;
  esac
  env -i \
    NSS_STATIC=1 \
    NSS_DIR="$LIBS_DIR/nss" \
    SQLCIPHER_STATIC=1 \
    SQLCIPHER_LIB_DIR="${LIBS_DIR}/sqlcipher/lib" \
    SQLCIPHER_INCLUDE_DIR="${LIBS_DIR}/sqlcipher/include" \
    PATH="${PATH}" \
    RUSTC_WRAPPER="${RUSTC_WRAPPER:-}" \
    SCCACHE_IDLE_TIMEOUT="${SCCACHE_IDLE_TIMEOUT:-}" \
    SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-}" \
    SCCACHE_ERROR_LOG="${SCCACHE_ERROR_LOG:-}" \
    RUST_LOG="${RUST_LOG:-}" \
    RUSTFLAGS="${RUSTFLAGS:-$DEFAULT_RUSTFLAGS}" \
    "$CARGO" "${BUILD_ARGS[@]}" --target "$TARGET"
}

set -euvx

# Intel iOS simulator
CFLAGS_x86_64_apple_ios="-target x86_64-apple-ios" \
  cargo_build x86_64-apple-ios

# Hardware iOS targets
cargo_build aarch64-apple-ios

# M1 iOS simulator.
CFLAGS_aarch64_apple_ios_sim="--target aarch64-apple-ios-sim" \
  cargo_build aarch64-apple-ios-sim

# TODO: would it be useful to also include desktop builds here?
# It might make it possible to run the Swift tests via `swift test`
# rather than through Xcode.

####
##
## 2) Stitch the individual builds together an XCFramework bundle.
##
####

TARGET_DIR="$REPO_ROOT/target"
XCFRAMEWORK_ROOT="$WORKING_DIR/$FRAMEWORK_FILENAME.xcframework"

# Start from a clean slate.

rm -rf "$XCFRAMEWORK_ROOT"

# Build the directory structure right for an individual framework.
# Most of this doesn't change between architectures.

COMMON="$XCFRAMEWORK_ROOT/common/$FRAMEWORK_NAME.framework"

mkdir -p "$COMMON/Modules"
cp "$WORKING_DIR/module.modulemap" "$COMMON/Modules/"

cp "$WORKING_DIR/DEPENDENCIES.md" "$COMMON/DEPENDENCIES.md"

mkdir -p "$COMMON/Headers"

# First we move the files that are common between both
# firefox-ios and Focus
cp "$WORKING_DIR/$FRAMEWORK_NAME.h" "$COMMON/Headers"
cp "$REPO_ROOT/components/rc_log/ios/RustLogFFI.h" "$COMMON/Headers"
cp "$REPO_ROOT/components/viaduct/ios/RustViaductFFI.h" "$COMMON/Headers"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/nimbus/src/nimbus.udl" -l swift -o "$COMMON/Headers"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/support/error/src/errorsupport.udl" -l swift -o "$COMMON/Headers"


# We now only move/generate the rest of the headers if we are generating a full
# iOS megazord
if [ -z $IS_FOCUS ]; then
  # TODO: https://github.com/mozilla/uniffi-rs/issues/1060
  # it would be neat if there was a single UniFFI command that would spit out
  # all of the generated headers for all UniFFIed dependencies of a given crate.
  # For now we generate the Swift bindings to get the headers as a side effect,
  # then delete the generated Swift code. Bleh.
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/crashtest/src/crashtest.udl" -l swift -o "$COMMON/Headers"
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/fxa-client/src/fxa_client.udl" -l swift -o "$COMMON/Headers"
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/logins/src/logins.udl" -l swift -o "$COMMON/Headers"
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/autofill/src/autofill.udl" -l swift -o "$COMMON/Headers"
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/push/src/push.udl" -l swift -o "$COMMON/Headers"
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/tabs/src/tabs.udl" -l swift -o "$COMMON/Headers"
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/places/src/places.udl" -l swift -o "$COMMON/Headers"
  $CARGO uniffi-bindgen generate "$REPO_ROOT/components/merino_client/src/merino_client.udl" -l swift -o "$COMMON/Headers"
fi
rm -rf "$COMMON"/Headers/*.swift



# Flesh out the framework for each architecture based on the common files.
# It's a little fiddly, because we apparently need to put all the simulator targets
# together into a single fat binary, but keep the hardware target separate.
# (TODO: we should try harder to see if we can avoid using `lipo` here, eliminating it
# would make the overall system simpler to understand).

# iOS hardware
mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64"
cp -r "$COMMON" "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework"
cp "$TARGET_DIR/aarch64-apple-ios/$BUILD_PROFILE/$LIB_NAME" "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/$FRAMEWORK_NAME"

# iOS simulator, with both platforms as a fat binary for mysterious reasons
mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator"
cp -r "$COMMON" "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework"
lipo -create \
  -output "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/$FRAMEWORK_NAME" \
  "$TARGET_DIR/aarch64-apple-ios-sim/$BUILD_PROFILE/$LIB_NAME" \
  "$TARGET_DIR/x86_64-apple-ios/$BUILD_PROFILE/$LIB_NAME"

# Set up the metadata for the XCFramework as a whole.

cp "$WORKING_DIR/Info.plist" "$XCFRAMEWORK_ROOT/Info.plist"
cp "$WORKING_DIR/DEPENDENCIES.md" "$XCFRAMEWORK_ROOT/DEPENDENCIES.md"

rm -rf "$XCFRAMEWORK_ROOT/common"

# Zip it all up into a bundle for distribution.

(cd "$WORKING_DIR" && zip -9 -r "$FRAMEWORK_FILENAME.xcframework.zip" "$FRAMEWORK_FILENAME.xcframework")
