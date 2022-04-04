#!/usr/bin/env bash

set -e

# shellcheck disable=SC2148
# Ensure the build toolchains are set up correctly for android builds.
#
# This file should be used via `./libs/verify-android-environment.sh`.

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: verify-android-environment.sh should be run from the root directory of the repo"
  exit 1
fi

echo "Verifying desktop-specific environment..."
# Android consumers are likely to also want to be able to run a quick
# `cargo build` for their desktop env, so verify that as well.
"$(pwd)/libs/verify-desktop-environment.sh"
echo ""

echo "Verifying android-specific environment..."
"$(pwd)/libs/verify-android-ci-environment.sh"

# Mac-specific checks, to help out with the M1 transition.
if [[ "$OSTYPE" == "darwin"* ]]; then
  java_arch=$(which java | xargs file | cut -f 5 -d ' ')
  system_arch=$(uname -m)
  if [ "$java_arch" != "$system_arch" ]; then
    echo "WARNING: mismatching Java (${java_arch}) and system (${system_arch}) architectures. Make sure this is intentional, or you may get strange build errors."
  fi
fi

echo "Looks good! Try building with ./gradlew assembleDebug"
