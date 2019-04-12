#!/usr/bin/env bash

set -euvx

if [ "$#" -ne 1 ]
then
    echo "Usage:"
    echo "./automation/check_megazord.sh <megazord_name>"
    exit 1
fi

MEGAZORD_NAME=$1

# For now just check x86_64 since we only run this for PRs
TARGET_ARCHS=("x86_64") # "x86" "arm64" "arm")
JNI_LIBS_TARGETS=("x86_64") # "x86" "arm64-v8a" "armeabi-v7a")
NM_BINS=("x86_64-linux-android-nm") # "i686-linux-android-nm" "aarch64-linux-android-nm" "arm-linux-androideabi-nm")
RUST_TRIPLES=("x86_64-linux-android") # "i686-linux-android" "aarch64-linux-android" "armv7-linux-androideabi")

FORBIDDEN_SYMBOL="viaduct_detect_reqwest_backend"
for i in "${!TARGET_ARCHS[@]}"; do
    NM="$ANDROID_NDK_TOOLCHAIN_DIR/${TARGET_ARCHS[$i]}-$ANDROID_NDK_API_VERSION/bin/${NM_BINS[$i]}"
    MEGAZORD_PATH="./target/${RUST_TRIPLES[i]}/release/lib$MEGAZORD_NAME.so"
    echo "\nTesting if $MEGAZORD_PATH contains the legacy/test-only HTTP stack\n"
    # Returns error status on failure, which will cause us to exit because of set -e.
    ./testing/err-if-symbol.sh "$NM" "$MEGAZORD_PATH" "$FORBIDDEN_SYMBOL"
done
