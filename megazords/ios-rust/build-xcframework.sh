#!/usr/bin/env bash
#
# This script builds the Rust crate in its directory into a staticlib XCFramework for iOS.

BUILD_PROFILE="release"
FRAMEWORK_NAME="MozillaRustComponents"

THIS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

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

set -euvx

# First, we build the rust code individually for each target architecture.

CARGO="$HOME/.cargo/bin/cargo"
BUILD_ARGS="build --manifest-path $MANIFEST_PATH $BUILD_PROFILE_FLAG --lib"

# Intel iOS simulator
CFLAGS_x86_64_apple_ios="-target x86_64-apple-ios" $CARGO $BUILD_ARGS --target x86_64-apple-ios

# Hardware iOS targets
$CARGO $BUILD_ARGS --target aarch64-apple-ios

# M1 iOS simulator (currently in Nightly only and requires to build `libstd`)
$CARGO +nightly $BUILD_ARGS -Z build-std --target aarch64-apple-ios-sim

# Then, we can stitch together an XCFramework bundle.
# It's a little fiddly, because we apparently need to put all the simulator targets
# together into a single fat binary, but keep the hardware target separate.

TARGET_DIR="$THIS_DIR/../../target"
XCFRAMEWORK_ROOT="$THIS_DIR/$FRAMEWORK_NAME.xcframework"

mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/Headers"
mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/Modules"
cp "$TARGET_DIR/aarch64-apple-ios/$BUILD_PROFILE/$LIB_NAME" "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/$FRAMEWORK_NAME"
cp "$THIS_DIR"/Headers/* "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/Headers/"
cp "$THIS_DIR"/Modules/* "$XCFRAMEWORK_ROOT/ios-arm64/$FRAMEWORK_NAME.framework/Modules/"


mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/Headers"
mkdir -p "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/Modules"
lipo -create \
  -output "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/$FRAMEWORK_NAME" \
  "$TARGET_DIR/aarch64-apple-ios-sim/$BUILD_PROFILE/$LIB_NAME" \
  "$TARGET_DIR/x86_64-apple-ios/$BUILD_PROFILE/$LIB_NAME"
cp "$THIS_DIR"/Headers/* "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/Headers/"
cp "$THIS_DIR"/Modules/* "$XCFRAMEWORK_ROOT/ios-arm64_x86_64-simulator/$FRAMEWORK_NAME.framework/Modules/"

cp "$THIS_DIR/Info.plist" "$XCFRAMEWORK_ROOT/Info.plist"

(cd "$THIS_DIR" && zip -r "$FRAMEWORK_NAME.xcframework.zip" "$FRAMEWORK_NAME.xcframework")
