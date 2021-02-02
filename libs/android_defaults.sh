#!/usr/bin/env bash

# Find the NDK.
pushd ..
NDK_VERSION=$(./gradlew -q printNdkVersion | tail -1)
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$NDK_VERSION"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
popd || exit

if [[ -z "${ANDROID_NDK_API_VERSION:-}" ]]; then
    export ANDROID_NDK_API_VERSION=21
    echo "The ANDROID_NDK_API_VERSION environment variable is not set. Defaulting to ${ANDROID_NDK_API_VERSION}"
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
    export NDK_HOST_TAG="darwin-x86_64"
elif [[ "$(uname -s)" == "Linux" ]]; then
    export NDK_HOST_TAG="linux-x86_64"
else
    echo "Unsupported OS."
fi
