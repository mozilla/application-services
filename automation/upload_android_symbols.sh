#!/usr/bin/env bash

set -euvx

if [ "$#" -ne 1 ]
then
    echo "Usage:"
    echo "./upload_android_symbols.sh <project path>"
    exit 1
fi

PROJECT_PATH=$1

source "libs/android_defaults.sh"

BREAKPAD_TOOLS="breakpad-tools-linux.zip"
BREAKPAD_TOOLS_URL="https://s3.amazonaws.com/getsentry-builds/getsentry/breakpad-tools/$BREAKPAD_TOOLS"
DUMP_SYMS_SHA256="4ce3d00251c4b213081399b7ee761830e1f285bff26dfd30a0c7ccbbb86e225b"
OUTPUT_FOLDER="crashreporter-symbols"
DUMP_SYMS_DIR="automation/symbols-generation/bin"

if [ ! -f "$DUMP_SYMS_DIR"/dump_syms ]; then
  curl -L -O "$BREAKPAD_TOOLS_URL"
  mkdir -p $DUMP_SYMS_DIR
  unzip $BREAKPAD_TOOLS -d $DUMP_SYMS_DIR dump_syms
  rm $BREAKPAD_TOOLS
  echo "${DUMP_SYMS_SHA256}  ${DUMP_SYMS_DIR}/dump_syms" | shasum -a 256 -c - || exit 2
fi

# Keep the 3 in sync.
TARGET_ARCHS=("x86_64" "x86" "arm64" "arm")
JNI_LIBS_TARGETS=("x86_64" "x86" "arm64-v8a" "armeabi-v7a")
OBJCOPY_BINS=("x86_64-linux-android-objcopy" "i686-linux-android-objcopy" "aarch64-linux-android-objcopy" "arm-linux-androideabi-objcopy")

rm -rf "$OUTPUT_FOLDER"
mkdir -p "$OUTPUT_FOLDER"

# 1. Generate the symbols.
for i in "${!TARGET_ARCHS[@]}"; do
  export OBJCOPY="$ANDROID_NDK_TOOLCHAIN_DIR/${TARGET_ARCHS[$i]}-$ANDROID_NDK_API_VERSION/bin/${OBJCOPY_BINS[$i]}"
  JNI_SO_PATH="$PROJECT_PATH/build/rustJniLibs/android/${JNI_LIBS_TARGETS[$i]}"
  for sofile in "$JNI_SO_PATH"/*.so; do
    python automation/symbols-generation/symbolstore.py -c -s . --vcs-info "$DUMP_SYMS_DIR"/dump_syms "$OUTPUT_FOLDER" "$sofile"
  done
done

# 2. Upload them.
pip install -r automation/symbols-generation/requirements.txt
python automation/symbols-generation/upload_symbols.py "$OUTPUT_FOLDER"
