# Protobuf Rust generator

You may ask yourself: why not use `prost-build` directly in `build.rs` scripts like everybody else does?
Well, Reasons, as usual.

- `prost-build` includes big binaries (> 4MB for each architecture) that we would not be able to check into mozilla-central.
- `protoc` is not even a dependency to build `mozilla-central`, therefore we would not be able to check in a "lite" version of `prost-build` that doesn't contain those binaries.

So instead, we use `prost-build` separately and check-in the Rust artifacts it generates. And that also makes the build faster, whoo-hoo.
