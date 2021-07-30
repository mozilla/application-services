#!/usr/bin/env bash

# shellcheck disable=SC2148
# Ensure the build toolchains are set up correctly for android builds.
#
# This file should be used via `./libs/verify-android-environment.sh`.

if [[ ! -f "$(pwd)/libs/build-all.sh" ]]; then
  echo "ERROR: verify-android-environment.sh should be run from the root directory of the repo"
  exit 1
fi

# Android consumers are likely to also want to be able to run a quick
# `cargo build` for their desktop env, so verify that as well.
"$(pwd)/libs/verify-desktop-environment.sh"

"$(pwd)/libs/verify-android-ci-environment.sh"

echo "Looks good! Try building with ./gradlew assembleDebug"
