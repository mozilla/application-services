#!/bin/bash

set -euvx

abspath () { case "$1" in /*)printf "%s\\n" "$1";; *)printf "%s\\n" "$PWD/$1";; esac; }
export -f abspath

TARGET_ARCHS=("x86" "arm64" "arm")
TARGET_ARCHS_TOOLCHAINS=("i686-linux-android" "aarch64-linux-android" "arm-linux-androideabi")

# End of configuration.

if [ "$#" -ne 1 ]
then
    echo "Usage:"
    echo "./build-all-android.sh <OPENSSL_SRC_PATH>"
    exit 1
fi

OPENSSL_SRC_PATH=$1

echo "# Building openssl"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "android/""$ARCH""/openssl")
  if [ -d "$DIST_DIR" ]; then
    echo "$DIST_DIR already exists. Skipping building openssl."
  else
    ./build-openssl-android.sh "$OPENSSL_SRC_PATH" "$DIST_DIR" "$ANDROID_NDK_TOOLCHAIN_DIR/$ARCH-$ANDROID_NDK_API_VERSION" "${TARGET_ARCHS_TOOLCHAINS[$i]}" "$ANDROID_NDK_API_VERSION" || exit 1
  fi
done

echo "# Building openssl"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "android/""$ARCH""/openssl")
  ./build-openssl-android.sh "$OPENSSL_SRC_PATH" "$DIST_DIR" "${TOOLCHAINS_PATHS[$i]}" "${TARGET_ARCHS_TOOLCHAINS[$i]}" "$ANDROID_API_VERSION"
done
