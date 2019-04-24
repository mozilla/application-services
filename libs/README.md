## libs

This directory builds `openssl` for iOS, Android and desktop platforms.

### Usage

* `./build-all.sh ios` - Build for iOS
* `./build-all.sh android` - Build for Android
* `./build-all.sh desktop` - Build for Desktop


### Supported architectures

* Android: `TARGET_ARCHS=("x86" "x86_64" "arm64" "arm")`
* iOS: `TARGET_ARCHS=("x86_64" "arm64")`
