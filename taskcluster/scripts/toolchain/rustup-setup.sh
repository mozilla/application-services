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

# Rust
set -eux; \
    RUSTUP_PLATFORM='x86_64-unknown-linux-gnu'; \
    RUSTUP_VERSION='1.23.1'; \
    RUSTUP_SHA256='ed7773edaf1d289656bdec2aacad12413b38ad0193fff54b2231f5140a4b07c5'; \
    curl -sfSL --retry 5 --retry-delay 10 -O "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUSTUP_PLATFORM}/rustup-init"; \
    echo "${RUSTUP_SHA256} *rustup-init" | sha256sum -c -; \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --default-toolchain none; \
    rm rustup-init
export PATH=$HOME/.cargo/bin:$PATH

# This is not the right place for it, but also it's as good a place as any.
# Make sure git submodules are initialized.
git submodule update --init
