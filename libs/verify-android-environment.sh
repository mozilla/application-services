# Ensure the build toolchains are set up correctly for android builds.
#
# This file should be used via `./libs/verify-android-environment.sh`.

NDK_VERSION=15
RUST_TARGETS=("aarch64-linux-android" "armv7-linux-androideabi" "i686-linux-android" "x86_64-linux-android")

if [ ! -f "$(pwd)/libs/build-all.sh" ]; then
  echo "ERROR: verify-android-environment.sh should be run from the root directory of the repo"
  exit 1
fi

if [ -z "${ANDROID_HOME}" ]; then
  echo "Could not find Android SDK:"
  echo 'Please install the Android SDK and then set ANDROID_HOME.'
  exit 1
fi

if [ -z "${ANDROID_NDK_ROOT}" ]; then
  echo "Could not find Android NDK:"
  echo 'Please install the Android NDK r15c and then set ANDROID_NDK_ROOT.'
  exit 1
fi

INSTALLED_NDK_VERSION=$(sed -En -e 's/^Pkg.Revision[ \t]*=[ \t]*([0-9a-f]+).*/\1/p' ${ANDROID_NDK_ROOT}/source.properties)
if [ "${INSTALLED_NDK_VERSION}" != ${NDK_VERSION} ]; then
  echo "Wrong Android NDK version:"
  echo "Expected version ${NDK_VERSION}, got ${INSTALLED_NDK_VERSION}"
  exit 1
fi

INSTALLED_RUST_TARGETS=$(rustup target list)
for TARGET in "${RUST_TARGETS[@]}"
do
  if ! [ "$(echo "${INSTALLED_RUST_TARGETS}" | grep "${TARGET}")" ]; then
    echo "Missing Rust target: ${TARGET}"
    echo "Installing the required target, please hold on."
    rustup target add ${TARGET}
  fi
done

if [ -z "${ANDROID_NDK_TOOLCHAIN_DIR}" ]; then
  echo "Could not find Android NDK toolchain directory:"
  echo "1. Create a directory where to set up the toolchains (e.g. ~/.ndk-standalone-toolchains)."
  echo "2. Set ANDROID_NDK_TOOLCHAIN_DIR to this newly created directory."
  echo "3. Run setup_toolchains_local.sh in the libs/ directory."
  exit 1
fi

echo "Looks good! cd to libs/ and run ./build-all.sh android"
