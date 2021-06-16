#!/bin/bash

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

set -ex

WORKING_DIR=$(pwd)
FRAMEWORK_FOLDER_NAME="build/archives"
FRAMEWORK_NAME="NimbusStandalone"
FRAMEWORK_PATH="${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}/${FRAMEWORK_NAME}.xcframework"
FRAMEWORK_PATH_ZIP="${FRAMEWORK_PATH}.zip"
BUILD_SCHEME="NimbusStandalone"
SIMULATOR_ARCHIVE_PATH="${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}/simulator.xcarchive"
IOS_DEVICE_ARCHIVE_PATH="${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}/iOS.xcarchive"

rm -rf "${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}"
mkdir -p "${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}"

xcodebuild \
  archive \
  -workspace ./megazords/nimbus-ios/NimbusStandalone.xcodeproj/project.xcworkspace \
  ONLY_ACTIVE_ARCH=NO \
  -scheme ${BUILD_SCHEME} \
  -destination="generic/platform=iOS Simulator" \
  -archivePath "${SIMULATOR_ARCHIVE_PATH}" \
  -sdk iphonesimulator \
  SKIP_INSTALL=NO \
  BUILD_LIBRARY_FOR_DISTRIBUTION=YES

xcodebuild \
  archive \
  -workspace ./megazords/nimbus-ios/NimbusStandalone.xcodeproj/project.xcworkspace \
  -scheme ${BUILD_SCHEME} \
  -destination="generic/platform=iOS" \
  -archivePath "${IOS_DEVICE_ARCHIVE_PATH}" \
  -sdk iphoneos \
  SKIP_INSTALL=NO \
  BUILD_LIBRARY_FOR_DISTRIBUTION=YES

#find "${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}" -name "*.swiftinterface" -type f -print0 | xargs -0 -I% sed -i.bak 's/Glean\.//g' %
#find "${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}" -name "*.bak" -type f -delete

rm -rf "${FRAMEWORK_PATH}"
xcodebuild \
  -create-xcframework \
  -framework ${SIMULATOR_ARCHIVE_PATH}/Products/Library/Frameworks/${FRAMEWORK_NAME}.framework \
  -framework ${IOS_DEVICE_ARCHIVE_PATH}/Products/Library/Frameworks/${FRAMEWORK_NAME}.framework \
  -output "${FRAMEWORK_PATH}"

cd "${WORKING_DIR}/${FRAMEWORK_FOLDER_NAME}"
#cp "${WORKING_DIR}/DEPENDENCIES.md" .
rm -f "${FRAMEWORK_PATH_ZIP}"
zip -r "${FRAMEWORK_PATH_ZIP}" "${FRAMEWORK_NAME}.xcframework" #DEPENDENCIES.md
cp "${FRAMEWORK_PATH_ZIP}" "${WORKING_DIR}"
