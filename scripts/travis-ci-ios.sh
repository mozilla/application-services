#!/usr/bin/env bash

set -ex

rustup target add aarch64-apple-ios
rustup target add armv7-apple-ios
rustup target add i386-apple-ios
rustup target add x86_64-apple-ios
cargo install --force --git https://github.com/TimNN/cargo-lipo
cd fxa-client/sdks
carthage build --no-skip-current --verbose && carthage archive
rm -rf Carthage
mv FxAClient.framework.zip ../..
