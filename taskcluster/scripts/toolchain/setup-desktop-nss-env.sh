#!/bin/bash
# NSS environment setup for desktop builds
# Source this after copy-libs-dir.sh in pre-commands

set -eux

TARGET="${1:-}"

# Function to find libclang on Linux
find_libclang_linux() {
    # Try LLVM directories from newest to oldest (Ubuntu 20.04 has LLVM 10)
    for ver in 18 17 16 15 14 13 12 11 10; do
      path="/usr/lib/llvm-${ver}/lib"
      if [ -d "$path" ] && [ -n "$(find "$path" -name 'libclang.so*' 2>/dev/null)" ]; then
        export LIBCLANG_PATH="$path"
        echo "✓ Found libclang at $LIBCLANG_PATH (LLVM ${ver})"
        return 0
      fi
    done
    # Fallback to architecture-specific paths
    for path in "/usr/lib/x86_64-linux-gnu" "/usr/lib/aarch64-linux-gnu"; do
      if [ -d "$path" ] && [ -n "$(find "$path" -name 'libclang.so*' 2>/dev/null)" ]; then
        export LIBCLANG_PATH="$path"
        echo "✓ Found libclang at $LIBCLANG_PATH"
        return 0
      fi
    done
    echo "⚠ WARNING: Could not find libclang in standard locations"
    return 1
}

case "$TARGET" in
  x86_64-unknown-linux-gnu)
    NSS_DIR_PATH="$(pwd)/libs/desktop/linux-x86-64/nss"
    export NSS_DIR="$NSS_DIR_PATH"
    export NSS_STATIC=1
    find_libclang_linux
    echo "✓ NSS enabled for x86_64 Linux (glibc)"
    ;;

  x86_64-unknown-linux-musl)
    NSS_DIR_PATH="$(pwd)/libs/desktop/linux-x86-64/nss"
    export NSS_DIR="$NSS_DIR_PATH"
    export NSS_STATIC=1
    find_libclang_linux
    # musl needs extra help for bindgen to find headers
    export BINDGEN_EXTRA_CLANG_ARGS="--sysroot=/usr -isystem /usr/include/x86_64-linux-musl -isystem /usr/include"
    echo "✓ NSS enabled for x86_64 Linux (musl)"
    ;;

  aarch64-unknown-linux-gnu)
    NSS_DIR_PATH="$(pwd)/libs/desktop/linux-aarch64/nss"
    export NSS_DIR="$NSS_DIR_PATH"
    export NSS_STATIC=1
    find_libclang_linux
    echo "✓ NSS enabled for ARM64 Linux"
    ;;

  x86_64-pc-windows-gnu)
    NSS_DIR_PATH="$(pwd)/libs/desktop/win32-x86-64/nss"
    export NSS_DIR="$NSS_DIR_PATH"
    export NSS_STATIC=1
    find_libclang_linux
    echo "✓ NSS enabled for Windows x86_64"
    ;;

  aarch64-apple-darwin)
    if [ -d "$(pwd)/libs/desktop/darwin-aarch64/nss" ]; then
      NSS_DIR_PATH="$(pwd)/libs/desktop/darwin-aarch64/nss"
    else
      NSS_DIR_PATH="$(pwd)/libs/desktop/darwin/nss"
    fi
    export NSS_DIR="$NSS_DIR_PATH"
    export NSS_STATIC=1
    export LIBCLANG_PATH="/Library/Developer/CommandLineTools/usr/lib"
    echo "✓ NSS enabled for macOS ARM64"
    ;;

  x86_64-apple-darwin)
    if [ -d "$(pwd)/libs/desktop/darwin-x86-64/nss" ]; then
      NSS_DIR_PATH="$(pwd)/libs/desktop/darwin-x86-64/nss"
    else
      NSS_DIR_PATH="$(pwd)/libs/desktop/darwin/nss"
    fi
    export NSS_DIR="$NSS_DIR_PATH"
    export NSS_STATIC=1
    export LIBCLANG_PATH="/Library/Developer/CommandLineTools/usr/lib"
    echo "✓ NSS enabled for macOS x86_64"
    ;;

  *)
    echo "ERROR: Unsupported target '$TARGET'"
    exit 1
    ;;
esac
