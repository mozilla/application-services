#!/bin/bash

TASK_FOR="${1}"

export RUST_BACKTRACE='1'
# Don't block releases on compilation warnings.
if [ "$TASK_FOR" != "github-release" ]; then
    export RUSTFLAGS='-Dwarnings'
fi
export CARGO_INCREMENTAL='0'
export CI='1'
export CCACHE='sccache'
export RUSTC_WRAPPER='sccache'
export SCCACHE_IDLE_TIMEOUT='1200'
export SCCACHE_CACHE_SIZE='40G'
export SCCACHE_ERROR_LOG='/builds/worker/sccache.log'
export RUST_LOG='sccache=info'
# We are pinned at this rust stable version (see also ../.circleci/config.yml)
export RUST_STABLE_VERSION='1.43.0'

# Rust
set -eux; \
    RUSTUP_PLATFORM='x86_64-unknown-linux-gnu'; \
    RUSTUP_VERSION='1.18.3'; \
    RUSTUP_SHA256='a46fe67199b7bcbbde2dcbc23ae08db6f29883e260e23899a88b9073effc9076'; \
    curl -sfSL --retry 5 --retry-delay 10 -O "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUSTUP_PLATFORM}/rustup-init"; \
    echo "${RUSTUP_SHA256} *rustup-init" | sha256sum -c -; \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --default-toolchain none; \
    rm rustup-init
export PATH=$HOME/.cargo/bin:$PATH


rustup toolchain install "$RUST_STABLE_VERSION"
rustup default "$RUST_STABLE_VERSION"
rustup target add x86_64-linux-android i686-linux-android armv7-linux-androideabi aarch64-linux-android

cargo install --version 0.7.0 uniffi_bindgen

# This is not the right place for it, but also it's as good a place as any.
# Make sure git submodules are initialized.
git submodule update --init
