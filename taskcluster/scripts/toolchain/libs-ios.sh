#!/bin/bash
set -ex

# Clean out files from the previous run
rm -fr ../gyp "$HOME/Library/Python/" "${HOME:?}/bin"

# Add cargo/rust to PATH
# shellcheck source=/dev/null
source "$HOME"/.cargo/env

# Install ninja, gyp, and xcpretty
NINJA_DOWNLOAD_URL=https://github.com/ninja-build/ninja/releases/download/v1.11.1/ninja-mac.zip
NINJA_SHA256=482ecb23c59ae3d4f158029112de172dd96bb0e97549c4b1ca32d8fad11f873e
curl -OL "$NINJA_DOWNLOAD_URL"
echo "${NINJA_SHA256}  ninja-mac.zip" | shasum -a 256 -c -
unzip ninja-mac.zip -d "$HOME/bin"
rm ninja-mac.zip
gem install --user-install --bindir "$HOME"/bin xcpretty
pushd ..
git clone https://chromium.googlesource.com/external/gyp.git
pip3 install -v --user ./gyp six
popd
export PATH="$HOME/bin:$HOME/Library/Python/3.7/bin:$PATH"

HOMEBREW_TARBALL=https://github.com/Homebrew/brew/archive/refs/tags/4.0.16.tar.gz
HOMEBREW_SHA256=ffa11c67aeb3182360b0d6fe82e2ac9e1c08f140e8d9aa5a1c19756d89f8e2f6
curl -OL "$HOMEBREW_TARBALL"
echo "${HOMEBREW_SHA256}  4.0.16.tar.gz" | shasum -a 256 -c -
tar -xzf 4.0.16.tar.gz --strip 1 -C "$HOME/bin"

export PATH="$HOME/bin/bin:$PATH"

brew update
brew install filosottile/musl-cross/musl-cross
brew install mingw-w64
ln -sf /usr/local/opt/musl-cross/bin/x86_64-linux-musl-gcc /usr/local/bin/musl-gcc
# Build the libs
pushd libs
./build-all.sh ios
popd

# TODO: re-enable this once we split out the toolchain tasks from swift-build
# mkdir -p "$UPLOAD_DIR"
# tar -czf "$UPLOAD_DIR"/ios.tar.gz libs/ios
