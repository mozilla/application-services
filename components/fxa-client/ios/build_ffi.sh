# build fxa-client lipoed library
export PATH="$HOME/.cargo/bin:$PATH"
cargo lipo --xcode-integ --package fxaclient_ffi --manifest-path ../Cargo.toml

