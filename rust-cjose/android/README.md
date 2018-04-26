## Android

Generate this directory using

```bash
${NDK_HOME}/build/tools/make_standalone_toolchain.py --api 26 --arch arm64 --install-dir NDK/arm64
${NDK_HOME}/build/tools/make_standalone_toolchain.py --api 26 --arch arm --install-dir NDK/arm
${NDK_HOME}/build/tools/make_standalone_toolchain.py --api 26 --arch x86 --install-dir NDK/x86
```


Create a `cargo-config.toml` and move it to your `~/.cargo/config`:

```
[target.aarch64-linux-android]
ar = "<project path>/android/NDK/arm64/bin/aarch64-linux-android-ar"
linker = "<project path>/android/NDK/arm64/bin/aarch64-linux-android-clang"

[target.armv7-linux-androideabi]
ar = "<project path>/android/NDK/arm/bin/arm-linux-androideabi-ar"
linker = "<project path>/android/NDK/arm/bin/arm-linux-androideabi-clang"

[target.i686-linux-android]
ar = "<project path>/android/NDK/x86/bin/i686-linux-android-ar"
linker = "<project path>/android/NDK/x86/bin/i686-linux-android-clang"
```

Don't forget the thing:
```
TARGET_AR="/Users/vladikoff/dev/rust-cjose/android/NDK/arm64/bin/aarch64-linux-android-ar"
```