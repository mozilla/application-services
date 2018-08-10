#!/usr/bin/env bash

set -euvx

apt-get update -qq && apt-get install zip -y

mkdir -p .cargo
yes | cp -rf scripts/taskcluster-cargo-config .cargo/config
pushd libs/ && ./build-all.sh android && ./build-patchelf.sh && popd

declare -A android_targets
android_targets=(
  ["x86"]="i686-linux-android"
  ["arm"]="armv7-linux-androideabi"
  ["arm64"]="aarch64-linux-android"
)

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

cd /build/application-services

ORIG_PATH="$PATH"
for target in "${selected_targets[@]}"
do
  PATH="$ANDROID_NDK_TOOLCHAIN_DIR/$target-$ANDROID_NDK_API_VERSION/bin:$ORIG_PATH"
  echo "Building target $target. Signature: ${android_targets[$target]}"
  OPENSSL_STATIC=1 OPENSSL_DIR=/build/application-services/libs/android/$target/openssl \
    cargo +beta build -p fxa-client-ffi --target ${android_targets[$target]} --release
  mkdir -p fxa-client/$target
  cp target/${android_targets[$target]}/release/libfxa_client.so fxa-client/$target
  ./libs/bin/patchelf --set-soname libfxa_client.so fxa-client/$target/libfxa_client.so
done

# Because Android needs the lib to be in a armeabi-v7a dir.
mv fxa-client/arm fxa-client/armeabi-v7a

cd fxa-client && zip -r fxa_client_android.zip * && cd ..
