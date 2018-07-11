## libs

This directory builds `openssl` for iOS, Android and desktop platforms.

### Usage

* `./build-all.sh ios` - Build for iOS
* `./build-all.sh android` - Build for Android
* `./build-all.sh desktop` - Build for Desktop


### Supported Arch

* Android: `TARGET_ARCHS=("x86" "arm64" "arm")`
* iOS: `TARGET_ARCHS=("i386" "x86_64" "armv7" "arm64")`
