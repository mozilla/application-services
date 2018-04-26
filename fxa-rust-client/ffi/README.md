# FxA Rust Client FFI

## iOS build

- Make sure you have the nightly compiler in order to get LLVM Bitcode generation.
- Install [cargo-lipo](https://github.com/TimNN/cargo-lipo/#installation).
- Build with: `OPENSSL_DIR=/usr/local/opt/openssl cargo +nightly lipo --release`

Have a look at [fxa-client-ios](https://github.com/eoger/fxa-client-ios) for a usage example.
