## libs

This directory builds `openssl`, `cjose` and `jansson` for iOS and Android.
`jansson` is required for `cjose`.

### Usage

* `./build-all.sh` - Build for both iOS and Android
* `./build-all.sh ios` - Just iOS
* `./build-all.sh android` - Just Android


### Supported Arch

* Android: `TARGET_ARCHS=("x86" "arm64" "arm")`
* iOS: `TARGET_ARCHS=("i386" "x86_64" "armv7" "arm64")`
