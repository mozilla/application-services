SRCROOT=`pwd`
pushd "$SRCROOT"/../../../libs
env -i PATH="$PATH" HOME="$HOME" ./build-all.sh ios
popd
cd "$SRCROOT"/../ffi
# We can't use cargo lipo because we can't link to universal libraries :(
# https://github.com/rust-lang/rust/issues/55235
LIBS_ARCHS=("x86_64" "arm64")
IOS_TRIPLES=("x86_64-apple-ios" "aarch64-apple-ios")
for i in "${!LIBS_ARCHS[@]}"; do
    LIB_ARCH=${LIBS_ARCHS[$i]}
    env -i PATH="$PATH" \
    OPENSSL_STATIC=1 \
    OPENSSL_DIR="$SRCROOT"/../../../libs/ios/$LIB_ARCH/openssl \
    SQLCIPHER_LIB_DIR="$SRCROOT"/../../../libs/ios/$LIB_ARCH/sqlcipher/lib \
    SQLCIPHER_INCLUDE_DIR="$SRCROOT"/../../../libs/ios/$LIB_ARCH/sqlcipher/include \
    "$HOME"/.cargo/bin/cargo build --lib --release  --target ${IOS_TRIPLES[$i]}
done
mkdir -p "$SRCROOT"/../../../target/universal/release
lipo -create -output "$SRCROOT"/../../../target/universal/release/liblogins_ffi.a \
"$SRCROOT"/../../../target/x86_64-apple-ios/release/liblogins_ffi.a \
"$SRCROOT"/../../../target/aarch64-apple-ios/release/liblogins_ffi.a \
