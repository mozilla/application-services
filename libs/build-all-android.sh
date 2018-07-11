#!/bin/bash

set -e

NDK_VERSION="17"
ANDROID_API_VERSION="26"
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

if [ -d "android" ]; then
  echo "android folder already exists. Skipping build."
  exit 0
fi

NDK_PATH="/tmp/android-ndk-r$NDK_VERSION"

echo "# Preparing build environment"

if [ -d "$NDK_PATH" ]; then
  echo "Using existing NDK"
else
  if [[ "$OSTYPE" == "linux-gnu" ]]; then
    NDK_ZIP="android-ndk-r""$NDK_VERSION""-linux-x86_64.zip"
  elif [[ "$OSTYPE" == "darwin"* ]]; then
    NDK_ZIP="android-ndk-r""$NDK_VERSION""-darwin-x86_64.zip"
  else
    echo "Unsupported platform!"
    exit 1
  fi
  curl -O "https://dl.google.com/android/repository/""$NDK_ZIP"
  unzip "$NDK_ZIP" -d /tmp
  rm -f "$NDK_ZIP"
fi

declare -a TOOLCHAINS_PATHS
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  TOOLCHAIN_DIR="/tmp/android-toolchain-""$ARCH"
  if ! [ -d "$TOOLCHAIN_DIR" ]; then
    "$NDK_PATH""/build/tools/make-standalone-toolchain.sh" --arch="$ARCH" --install-dir="$TOOLCHAIN_DIR" --platform="android-""$ANDROID_API_VERSION"
  fi
  TOOLCHAINS_PATHS[$i]=$TOOLCHAIN_DIR
done

echo "# Building openssl"
for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "android/""$ARCH""/openssl")
  ./build-openssl-android.sh "$OPENSSL_SRC_PATH" "$DIST_DIR" "${TOOLCHAINS_PATHS[$i]}" "${TARGET_ARCHS_TOOLCHAINS[$i]}" "$ANDROID_API_VERSION"
done
