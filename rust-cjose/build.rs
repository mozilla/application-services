use std::env;

extern crate cc;
extern crate pkg_config;


fn main() {
    env::set_var("CFLAGS", "-I/usr/local/opt/openssl/include -L/usr/local/opt/openssl/lib");
    //env::set_var("CC", "arm-linux-gnueabihf-gcc");
    cc::Build::new()
        .file("src/cjose/version.c")
        //.shared_flag(true)
        //.pic(true)
        //.target("aarch64-linux-android")
        .target("armv7-linux-androideabi")
        // this is the alias for GCC on my computer
        //.compiler("gcc")
        //.compiler("/Users/vladikoff/dev/rust-cjose/android/NDK/arm64/bin/aarch64-linux-android-gcc")
        .compiler("/Users/vladikoff/dev/rust-cjose/android/NDK/arm/bin/arm-linux-androideabi-gcc")
        .compile("version.so");
}
