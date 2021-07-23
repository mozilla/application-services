#!/usr/bin/env bash
#
# This script builds the Rust crate in its directory into a staticlib XCFramework for iOS.

BUILD_PROFILE="release"
FRAMEWORK_NAME="MozillaRustComponents"

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$( dirname "$( dirname "$THIS_DIR" )" )"

MANIFEST_PATH="$THIS_DIR/Cargo.toml"
if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "Could not locate Cargo.toml relative to script"
  exit 1
fi

CRATE_NAME=`grep --max-count=1 '^name =' $MANIFEST_PATH | cut -d '"' -f 2`
if [[ -z "$CRATE_NAME" ]]; then
  echo "Could not determine crate name from $MANIFEST_PATH"
  exit 1
fi
LIB_NAME="lib${CRATE_NAME}.a"

BUILD_PROFILE_FLAG=
if [[ "$BUILD_PROFILE" = "release" ]]; then
    BUILD_PROFILE_FLAG=--release
fi

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
BUILD_ARGS="build --manifest-path $MANIFEST_PATH $BUILD_PROFILE_FLAG --lib"
LIBS_DIR="$REPO_ROOT/libs"

cargo_build () {
  TARGET=$1
  CHANNEL="${2:-}"
  case $TARGET in
    x86_64*)
      LIBS_ARCH="x86_64";;
    aarch64*)
      LIBS_ARCH="arm64";;
    *)
      echo "Unexpected target architecture: $TARGET" && exit 1;;
  esac
  env -i \
    NSS_STATIC=1 \
    NSS_DIR="$LIBS_DIR/ios/$LIBS_ARCH/nss" \
    SQLCIPHER_STATIC=1 \
    SQLCIPHER_LIB_DIR="${LIBS_DIR}/ios/${LIBS_ARCH}/sqlcipher/lib" \
    SQLCIPHER_INCLUDE_DIR="${LIBS_DIR}/ios/${LIBS_ARCH}/sqlcipher/include" \
    PATH="${PATH}" \
    RUSTC_WRAPPER="${RUSTC_WRAPPER:-}" \
    SCCACHE_IDLE_TIMEOUT="${SCCACHE_IDLE_TIMEOUT:-}" \
    SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-}" \
    SCCACHE_ERROR_LOG="${SCCACHE_ERROR_LOG:-}" \
    RUST_LOG="${RUST_LOG:-}" \
    $CARGO $CHANNEL $BUILD_ARGS --target $TARGET
}

set -euvx

# Intel iOS simulator
# TODO: why is the env var necessary?
CFLAGS_x86_64_apple_ios="-target x86_64-apple-ios" \
  cargo_build x86_64-apple-ios

# Hardware iOS targets
cargo_build aarch64-apple-ios

# M1 iOS simulator (currently in Nightly only and requires to build `libstd`)
BUILD_ARGS="$BUILD_ARGS -Z build-std"
cargo_build aarch64-apple-ios-sim +nightly

####
##
## 2) Stitch the individual builds together an XCFramework bundle.
##
####

TARGET_DIR="$REPO_ROOT/target"
XCFRAMEWORK_ROOT="$THIS_DIR/$FRAMEWORK_NAME.xcframework"

# Build the directory structure right for an individual framework.
# Most of this doesn't change between architectures.

COMMON="$XCFRAMEWORK_ROOT/common/$FRAMEWORK_NAME.framework"

mkdir -p "$COMMON/Modules"
cp "$THIS_DIR/module.modulemap" "$COMMON/Modules/"

mkdir -p "$COMMON/Headers"
cp "$REPO_ROOT/megazords/ios/RustLog/RustLogFFI.h" "$COMMON/Headers"
cp "$REPO_ROOT/megazords/ios/Viaduct/RustViaductFFI.h" "$COMMON/Headers"
# TODO: it would be neat if there was a single UniFFI command that would spit out
# all of the generates headers for all UniFFIed dependencies of a given crate.
# For now we generate the Swift bindings to get the headers as a side effect,
# then delete the generated Swift code. Bleh.
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/nimbus/src/nimbus.udl" -l swift -o "$COMMON/Headers"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/crashtest/src/crashtest.udl" -l swift -o "$COMMON/Headers"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/fxa-client/src/fxa_client.udl" -l swift -o "$COMMON/Headers"
$CARGO uniffi-bindgen generate "$REPO_ROOT/components/logins/src/logins.udl" -l swift -o "$COMMON/Headers"
rm -rf "$COMMON"/Headers/*.swift

# Flesh out the framework for each architecture based on the common files.
# It's a little fiddly, because we apparently need to put all the simulator targets
# together into a single fat binary, but keep the hardware target separate.
# (TODO: we should try harder to see if we can avoid using `lipo` here, eliminating it
# would make the overall system simpler to understand).

mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64"
cp -r "$COMMON" "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/"
cp "$TARGET_DIR/aarch64-apple-ios/$BUILD_PROFILE/$LIB_NAME" "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/$FRAMEWORK_NAME"

mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator"
cp -r "$COMMON" "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/"
lipo -create \
  -output "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/$FRAMEWORK_NAME" \
  "$TARGET_DIR/aarch64-apple-ios-sim/$BUILD_PROFILE/$LIB_NAME" \
  "$TARGET_DIR/x86_64-apple-ios/$BUILD_PROFILE/$LIB_NAME"

# Set up the metadata for the XCFramework as a whole.

cp "$THIS_DIR/Info.plist" "$XCFRAMEWORK_ROOT/Info.plist"
rm -rf "$XCFRAMEWORK_ROOT/common"

# Zip it all up into a bundle for distribution.

(cd "$THIS_DIR" && zip -r "$FRAMEWORK_NAME.xcframework.zip" "$FRAMEWORK_NAME.xcframework")
rm -rf "$XCFRAMEWORK_ROOT"
