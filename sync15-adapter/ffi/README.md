# Sync 1.5 Client FFI

This README is shamelessly stolen from the one in the fxa client directory

## iOS build

- Make sure you have the nightly compiler in order to get LLVM Bitcode generation.
- Install [cargo-lipo](https://github.com/TimNN/cargo-lipo/#installation).
- Build with: `OPENSSL_DIR=/usr/local/opt/openssl cargo +nightly lipo --release`
