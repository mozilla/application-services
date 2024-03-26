#!/bin/bash
set -ex

# Clean out files from the previous run
rm -fr gyp "$HOME/Library/Python/" "${HOME:?}/bin"

# shellcheck source=/dev/null
source vcs/taskcluster/scripts/setup-mac-worker.sh

# Install ninja, gyp, and xcpretty
NINJA_DOWNLOAD_URL=https://github.com/ninja-build/ninja/releases/download/v1.11.1/ninja-mac.zip
NINJA_SHA256=482ecb23c59ae3d4f158029112de172dd96bb0e97549c4b1ca32d8fad11f873e
curl -OL "$NINJA_DOWNLOAD_URL"
echo "${NINJA_SHA256}  ninja-mac.zip" | shasum -a 256 -c -
unzip ninja-mac.zip -d "$HOME/bin"
rm ninja-mac.zip
gem install --user-install --bindir "$HOME"/bin xcpretty
git clone https://chromium.googlesource.com/external/gyp.git
pip3 install -v --user ./gyp six
export PATH="$HOME/bin:$HOME/Library/Python/3.11/bin:$PATH"

# Build the libs
cd vcs/libs
./build-all.sh ios
cd ..

mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/ios.tar.gz libs/ios
