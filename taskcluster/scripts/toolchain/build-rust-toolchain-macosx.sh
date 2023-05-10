#!/bin/bash
#
# Build the Rust toolchain for other tasks:
#
#   - The "Rust toolchain" includes all targets/components listed in our `rust-toolchain` file
#   - It also includes some targets needed for cross-compilation
#   - run-task from taskgraph handles uploading files from UPLOAD_DIR to the public artifacts directory at the end of the task
#   - run-task also handles downloading and untaring those artifacts to the fetches directory at the start of other tasks (if `rust` is included in `fetches:toolchain`)
#   - `setup-fetched-rust-toolchain.sh` handles copying the files to the correct directories

set -ex

# UPLOAD_DIR is not set for the generic worker, so we need to set it ourselves
# FIXME: what's the right way to get this value?
UPLOAD_DIR="${PWD}/../public/build"
# Clear out any existing Rust files
rm -fr ~/.cargo ~/.rustup
# Install rustup
RUSTUP_PLATFORM=x86_64-apple-darwin
RUSTUP_VERSION=1.24.1
RUSTUP_SHA256=d53e8000c8663e1704a2071f7042be917bc90cbc89c11e11c5dfdcb35b84c00e
curl -sfSL --retry 5 --retry-delay 10 -O "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUSTUP_PLATFORM}/rustup-init"
echo "${RUSTUP_SHA256} *rustup-init" | shasum -a 256 -c -
chmod +x rustup-init
./rustup-init -y --no-modify-path
rm rustup-init
# shellcheck source=/dev/null
source "$HOME"/.cargo/env
# So long as this is executed after the checkout it will use the version specified in rust-toolchain.yaml
rustup update
rustup target add aarch64-apple-darwin

# Tar everything into UPLOAD_DIR
cd "$HOME"
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/rust-osx.tar.gz .rustup .cargo
