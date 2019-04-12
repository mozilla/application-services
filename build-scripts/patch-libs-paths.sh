#!/usr/bin/env bash
set -euvx

# This script patches our NSS .dylibs to load each other using @rpath instead of
# @executable_path which is incorrect.

# It should be called inside XCode as it reads env variables set by it

pushd ${TARGET_BUILD_DIR}/${TARGET_NAME}.framework
for binary in *.dylib; do
  install_name_tool -id @rpath/${binary} ${binary}
  for lib in *.dylib; do
    install_name_tool -change @executable_path/${lib} @rpath/${lib} ${binary}
  done
done
popd
