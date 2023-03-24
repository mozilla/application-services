#!/bin/bash
# This script is sourced by the `build-*-android.sh` scripts.  The shbang above
# is to make shellcheck happy.

export AR="${TOOLCHAIN_PATH}/bin/llvm-ar"
export CC="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}${ANDROID_NDK_API_VERSION}-clang"
export CXX="${TOOLCHAIN_PATH}/bin/${TOOLCHAIN}${ANDROID_NDK_API_VERSION}-clang++"
# For 32-bit ARM, the compiler is prefixed with armv7a-linux-androideabi
if [[ "${TOOLCHAIN}" == "arm-linux-androideabi" ]]; then
  export CC="${TOOLCHAIN_PATH}/bin/armv7a-linux-androideabi${ANDROID_NDK_API_VERSION}-clang"
  export CXX="${TOOLCHAIN_PATH}/bin/armv7a-linux-androideabi${ANDROID_NDK_API_VERSION}-clang++"
fi
export LD="${TOOLCHAIN_PATH}/bin/ld"
export NM="${TOOLCHAIN_PATH}/bin/llvm-nm"
export RANLIB="${TOOLCHAIN_PATH}/bin/llvm-ranlib"
export READELF="${TOOLCHAIN_PATH}/bin/llvm-readelf"

