# Sync 1.5 Client FFI

This README is shamelessly stolen from the one in the fxa client directory

## iOS build

- Make sure you have the nightly compiler in order to get LLVM Bitcode generation.
- Install [cargo-lipo](https://github.com/TimNN/cargo-lipo/#installation).
- Build with: `OPENSSL_DIR=/usr/local/opt/openssl cargo +nightly lipo --release`
- Update cargo FFI header (after installing cbindgen) with `cbindgen sync15-adapter -o sync15-adapter/ffi/sync_adapter.h`. (Note: It's safe to ignore warnings about types or functions that aren't exported).

