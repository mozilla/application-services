#!/usr/bin/env bash

set -ex

ANDROID_API_VERSION="26"
NDK_VERSION="17"

declare -A android_targets
android_targets=(
  ["x86"]="i686-linux-android"
  ["arm"]="armv7-linux-androideabi"
  ["arm64"]="aarch64-linux-android"
)

NDK_PATH="/tmp/android-ndk-r$NDK_VERSION"
if ! [ -d "$NDK_PATH" ]; then
  if [[ "$OSTYPE" == "linux-gnu" ]]; then
    NDK_ZIP="android-ndk-r""$NDK_VERSION""-linux-x86_64.zip"
  elif [[ "$OSTYPE" == "darwin"* ]]; then
    NDK_ZIP="android-ndk-r""$NDK_VERSION""-darwin-x86_64.zip"
  else
    echo "Unsupported platform!"
    exit 1
  fi
  curl -O "https://dl.google.com/android/repository/""$NDK_ZIP"
  unzip -o "$NDK_ZIP" -d /tmp
  rm -f "$NDK_ZIP"
fi

if [ "$#" -eq 0 ]
then
  selected_targets=(x86 arm arm64)
else
  for target_arg in "$@"
  do
    [[ -z "${android_targets[$target_arg]+yes}" ]] && echo "Unrecognized target $target_arg. Supported targets: ${!android_targets[@]}" && exit 1
    selected_targets=("${selected_targets[@]}" $target_arg)
  done
fi

echo "Building selected targets: ${selected_targets[@]}."

for target in "${selected_targets[@]}"
do
  rustup target add ${android_targets[$target]}
  echo "Creating toolchain for ${android_targets[$target]}"
  TOOLCHAIN_DIR="/tmp/android-toolchain-$target"
  "$NDK_PATH/build/tools/make-standalone-toolchain.sh" --arch="$target" --install-dir="$TOOLCHAIN_DIR" --platform="android-$ANDROID_API_VERSION" --force
  PATH="$TOOLCHAIN_DIR/bin:$PATH"
  echo "Building target $target. Signature: ${android_targets[$target]}"
  JANSSON_DIR="$PWD"/libs/android/$target/jansson/lib \
  OPENSSL_STATIC=0 OPENSSL_DIR="$PWD"/libs/android/$target/openssl \
  CJOSE_DIR="$PWD"/libs/android/$target/cjose/lib \
  cargo build -p fxa-client-ffi --target ${android_targets[$target]} --release
  mkdir -p fxa-client/$target
  cp target/${android_targets[$target]}/release/libfxa_client.so fxa-client/$target
  mkdir -p fxa-client-deps/$target
  cp -r libs/android/$target/*/lib/*.so fxa-client-deps/$target
done

# Because Android needs the lib to be in a armeabi dir.
mv fxa-client/arm fxa-client/armeabi
mv fxa-client-deps/arm fxa-client-deps/armeabi
