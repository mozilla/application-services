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

# Install rustup
RUSTUP_PLATFORM='x86_64-unknown-linux-gnu'
RUSTUP_VERSION='1.24.1'
RUSTUP_SHA256='fb3a7425e3f10d51f0480ac3cdb3e725977955b2ba21c9bdac35309563b115e8'
curl -sfSL --retry 5 --retry-delay 10 -O "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUSTUP_PLATFORM}/rustup-init"
echo "${RUSTUP_SHA256} *rustup-init" | sha256sum -c -
chmod +x rustup-init
./rustup-init -y --no-modify-path --default-toolchain none

# cd to the app-services directory, so that rustup will see our `rust-toolchain.toml` file
cd /builds/worker/checkouts/src
# `rustup --version` causes the compilers and components from `rust-toolchain.toml` to be installed
rustup --version
# cross-compilation targets
rustup target add x86_64-apple-darwin
rustup target add x86_64-pc-windows-gnu

# Tar everything into UPLOAD_DIR
cd "$HOME"
mkdir -p "$UPLOAD_DIR"
tar -czf "$UPLOAD_DIR"/rust.tar.gz .rustup .cargo
