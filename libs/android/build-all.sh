#!/bin/bash

set -e

JANSSON_VERSION="2.11"
OPENSSL_VERSION="1.0.2o"
CJOSE_VERSION="0.6.1"
NDK_VERSION="17"
ANDROID_API_VERSION="26"
TARGET_ARCHS=("x86" "arm64" "arm")
TARGET_ARCHS_TOOLCHAINS=("i686-linux-android" "aarch64-linux-android" "arm-linux-androideabi")

# End of configuration.

abspath () { case "$1" in /*)printf "%s\n" "$1";; *)printf "%s\n" "$PWD/$1";; esac; }
NDK_PATH=$(abspath "android-ndk-r""$NDK_VERSION")


echo "# Preparing build environment"

if [ -d "$NDK_PATH" ]; then
  echo "Using existing NDK"
else
  #TODO: replacing "darwin" by "linux" would allow this script to run on linux potentially.
  NDK_ZIP="android-ndk-r""$NDK_VERSION""-darwin-x86_64.zip"
  curl -O "https://dl.google.com/android/repository/""$NDK_ZIP"
  unzip "$NDK_ZIP"
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


echo "# Building jansson"

JANSSON="jansson-${JANSSON_VERSION}"
rm -rf "${JANSSON}"
if [ ! -e "${JANSSON}.tar.gz" ]; then
  echo "Downloading ${JANSSON}.tar.gz"
  curl -O "http://www.digip.org/jansson/releases/${JANSSON}.tar.gz"
else
  echo "Using ${JANSSON}.tar.gz"
fi
tar xfz "${JANSSON}.tar.gz"
JANSSON_SRC_PATH=$(abspath $JANSSON)

for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "$ARCH""/jansson")
  ./build-jansson.sh $JANSSON_SRC_PATH $DIST_DIR ${TOOLCHAINS_PATHS[$i]} ${TARGET_ARCHS_TOOLCHAINS[$i]} $ANDROID_API_VERSION
done

echo "Cleaning up"
rm -rf ${JANSSON_SRC_PATH}


echo "# Building openssl"

OPENSSL="openssl-${OPENSSL_VERSION}"
rm -rf "${OPENSSL}"
if [ ! -e "${OPENSSL}.tar.gz" ]; then
  echo "Downloading ${OPENSSL}.tar.gz"
  curl -O "https://www.openssl.org/source/""${OPENSSL}"".tar.gz"
else
  echo "Using ${OPENSSL}.tar.gz"
fi
tar xfz "${OPENSSL}.tar.gz"
OPENSSL_SRC_PATH=$(abspath $OPENSSL)

for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "$ARCH""/openssl")
  ./build-openssl.sh $OPENSSL_SRC_PATH $DIST_DIR ${TOOLCHAINS_PATHS[$i]} ${TARGET_ARCHS_TOOLCHAINS[$i]} $ANDROID_API_VERSION
done

echo "Cleaning up"
rm -rf ${OPENSSL_SRC_PATH}


echo "# Building cjose"

CJOSE="cjose-${CJOSE_VERSION}"
rm -rf "${CJOSE}"
if [ ! -e "${CJOSE}.tar.gz" ]; then
  echo "Downloading ${CJOSE}.tar.gz"
  curl -L "https://github.com/cisco/cjose/archive/${CJOSE_VERSION}.tar.gz" -o "${CJOSE}.tar.gz"
else
  echo "Using ${CJOSE}.tar.gz"
fi
tar xfz "${CJOSE}.tar.gz"
CJOSE_SRC_PATH=$(abspath $CJOSE)

for i in "${!TARGET_ARCHS[@]}"; do
  ARCH=${TARGET_ARCHS[$i]}
  DIST_DIR=$(abspath "$ARCH""/cjose")
  JANSSON_DIR=$(abspath "$ARCH""/jansson")
  OPENSSL_DIR=$(abspath "$ARCH""/openssl")
  ./build-cjose.sh $CJOSE_SRC_PATH $DIST_DIR ${TOOLCHAINS_PATHS[$i]} ${TARGET_ARCHS_TOOLCHAINS[$i]} $ANDROID_API_VERSION $JANSSON_DIR $OPENSSL_DIR
done

echo "Cleaning up"
rm -rf ${CJOSE_SRC_PATH}

echo "Done"
