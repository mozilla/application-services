# FxA Rust Client FFI

## iOS build

- Make sure you have the nightly compiler in order to get LLVM Bitcode generation.
- Install [cargo-lipo](https://github.com/TimNN/cargo-lipo/#installation).
- Build with: `cargo +nightly lipo --release`
