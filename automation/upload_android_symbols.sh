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

DUMP_SYMS_URL="https://queue.taskcluster.net/v1/task/KSunmnE8SWycOTSW2mT_AA/runs/0/artifacts/public/dump_syms"
DUMP_SYMS_SHA256="2176e51ed9da31966a716289f0bf46f59f60dea799cc8f85e086dd66d087b8d4"
OUTPUT_FOLDER="crashreporter-symbols"
DUMP_SYMS_DIR="automation/symbols-generation/bin"

if [ ! -f "$DUMP_SYMS_DIR"/dump_syms ]; then
  mkdir -p "$DUMP_SYMS_DIR"
  pushd "$DUMP_SYMS_DIR"
  curl -L -O -s --retry 5 "$DUMP_SYMS_URL"
  popd
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
