#!/usr/bin/env bash

if [ -z "${ANDROID_NDK_TOOLCHAIN_DIR:-}" ]; then
    export ANDROID_NDK_TOOLCHAIN_DIR="/tmp/android-ndk-toolchain"
    echo "The ANDROID_NDK_TOOLCHAIN_DIR env variable is not set. Defaulting to ${ANDROID_NDK_TOOLCHAIN_DIR}"
fi

if [ -z "${ANDROID_NDK_API_VERSION:-}" ]; then
    export ANDROID_NDK_API_VERSION=21
    echo "The ANDROID_NDK_API_VERSION env variable is not set. Defaulting to ${ANDROID_NDK_API_VERSION}"
fi
