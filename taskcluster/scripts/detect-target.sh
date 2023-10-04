#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# This script guesses the llvm target triple for the host machine.
#
# We're doing this in a shell script so it can be distributed with megazords
# to be deployed without a Rust compiler.
#
# With a rustc compiler present, we could determine definitively:
#
# % rustc -vV | sed -n 's|host: ||p' (credit: https://stackoverflow.com/a/69816610)
#
# This method is available by passing the --use-rustc argument.
#
# Otherwise, we want to be able to pick a binary from several already built. If we leave it
# to rustc, we may get the `linux-gnu` build instead of the `linux-musl` build.
#
# In this case, we use uname and some well placed `case` statements.
#
# If an argument is supplied, then it is treated as a library name, and a path to a shared object for that library
# is given. For example:
#
# % detect-target.sh foo-bar
# aarch64-apple-darwin/libfoo_bar.dylib
#

set -euo pipefail

detect_arch() {
  ARCH=$(uname -m)
  case $ARCH in
    aarch64) ARCH="aarch64" ;;
    arm64) ARCH="aarch64" ;;
    x86_64) ARCH="x86_64" ;;
    amd64) ARCH="x86_64" ;;
  esac
}

# initOS discovers the operating system for this system.
detect_os() {
  OS=$(uname|tr '[:upper:]' '[:lower:]')

  case "$OS" in
    # Minimalist GNU for Windows
    mingw*|cygwin*) OS='windows' ;;
    darwin*) OS='darwin' ;;
    linux*) OS='linux' ;;
  esac
}

detect_target() {
  local RUSTC
  local SED
  RUSTC=$(which rustc)
  SED=$(which sed)
  if [ -n "$RUSTC" ] && [ -n "$SED" ] ; then
    TARGET=$(rustc --version --verbose | sed -n 's|host: ||p' )
  fi
}

guess_target() {
  local DOUBLE=""
  case "$OS" in
    "darwin") DOUBLE="apple-darwin" ;;
    "linux") DOUBLE="unknown-linux-gnu" ;;
    "windows") DOUBLE="pc-windows-gnu" ;;
  esac
  TARGET="$ARCH-$DOUBLE"
}

guess_filename() {
  local lib="${1/-/_}"
  case "$OS" in
    "darwin") FILENAME="lib${lib}.dylib" ;;
    "linux") FILENAME="lib${lib}.so" ;;
    "windows") FILENAME="${lib}.dll" ;;
  esac
}

LIB_NAME=${1:-}

detect_arch
detect_os

TARGET=""
if [ "$LIB_NAME" == "--use-rustc" ] ; then
  shift
  LIB_NAME=${1:-}
  detect_target
fi

if [ -z "$TARGET" ] ; then
  guess_target
fi

if [ -n "$LIB_NAME" ]; then
  guess_filename "$LIB_NAME"
  echo -n "$TARGET/$FILENAME"
else
  echo -n "$TARGET"
fi
