#!/usr/bin/env bash

set -euvx

if [[ "${#}" -ne 2 ]]
then
    echo "Usage:"
    echo "./automation/generate_android_symbols.sh <project path> <output_path>"
    exit 1
fi

if [[ ! -f "$PWD/libs/android_defaults.sh" ]]
then
    echo "generate_android_symbols.sh must be executed from the root directory."
    exit 1
fi

PROJECT_PATH=${1}
OUTPUT_PATH=${2}

pushd libs
# shellcheck disable=SC1091
source "android_defaults.sh"
popd

OUTPUT_FOLDER="crashreporter-symbols"
DUMP_SYMS_DIR="automation/symbols-generation/bin"

if [[ ! -f "${DUMP_SYMS_DIR}/dump_syms" ]]; then
  tooltool.py --manifest=automation/symbols-generation/dump_syms.manifest --url=http://taskcluster/tooltool.mozilla-releng.net/ fetch
  chmod +x dump_syms
  mkdir -p "${DUMP_SYMS_DIR}"
  mv dump_syms "${DUMP_SYMS_DIR}"
fi

# Keep these 2 in sync.
TARGET_ARCHS=("x86_64" "x86" "arm64" "arm")
JNI_LIBS_TARGETS=("x86_64" "x86" "arm64-v8a" "armeabi-v7a")

rm -rf "${OUTPUT_FOLDER}"
mkdir -p "${OUTPUT_FOLDER}"

# Generate the symbols.
for i in "${!TARGET_ARCHS[@]}"; do
  export OBJCOPY="${ANDROID_NDK_ROOT}/toolchains/llvm/prebuilt/${NDK_HOST_TAG}/bin/llvm-objcopy"
  JNI_SO_PATH="${PROJECT_PATH}/build/rustJniLibs/android/${JNI_LIBS_TARGETS[${i}]}"
  for sofile in "${JNI_SO_PATH}"/*.so; do
    python3 automation/symbols-generation/symbolstore.py -c -s . --vcs-info "${DUMP_SYMS_DIR}"/dump_syms "${OUTPUT_FOLDER}" "${sofile}"
  done
done

tar zvcf "${OUTPUT_PATH}" "${OUTPUT_FOLDER}"
