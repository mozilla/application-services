# Building Application Services

## Introduction

First of all, let us remind the reader that were are not dealing with a classic Rust project where you can run `cargo build` and you are ready to go. Sorry.  
In fact, the project involves multiple build systems: `autoconf` for dependencies such as NSS or SQLCipher, `cargo` for the Rust common code, then either `gradle` (Android) or `XCode` (iOS) for the platform-specific wrappers.
The guide will make sure your system is configured appropriately to run these build systems without issues.

## Common (Desktop) build environment

Ensure this section works for you before jumping to the Android/iOS specific sections.

- System-level dependencies
  - gyp
    - Using apt: `apt install gyp`
    - [manual install](https://github.com/mogemimi/pomdog/wiki/How-to-Install-GYP)
  - [ninja-build](https://github.com/ninja-build/ninja/wiki/Pre-built-Ninja-packages)
  - tcl
    - Using apt: `apt install tclsh`
    - [manual install](https://www.tcl.tk/software/tcltk/)
  - python3
  - zlib
    - Using apt: `apt install zlib1g-dev`
    - Already installed on macOS by XCode.

Note that we have a guide if you happen to use [WSL](howtos/wsl-build-guide.md).

The following command will ensure your environment variables are set properly and build the dependencies needed by the project:

```
./libs/verify-desktop-environment.sh
```

You can then run the Rust tests using:

```
cargo test
```

## Android development

### Java 8

Using any other version than 8 __will__ mess up your build and is not supported.  
Please ensure `JAVA_HOME` points to the JDK 8 installation directory.

### Setting up the Android SDK

#### Through Android Studio

1. Download and install Android Studio and let it install the SDK for you.
1. In the "SDK Manager" window, find out the location of the SDK.
1. Set `ANDROID_SDK_ROOT` and `ANDROID_HOME` (technically deprecated 🤷‍♂️) to this location and add it to your rc file.

#### Manually

Visit https://developer.android.com/studio/ and download the "Command line tools".  
Install the SDK by executing:


```
> cd ~  
> mkdir -p android-sdk/cmdline-tools  
> cd android-sdk/cmdline-tools  
> unzip {path-to.zip}  
> # Don't forget to write these to your rc file!
> export ANDROID_SDK_ROOT=$HOME/android-sdk
> export ANDROID_HOME=$ANDROID_SDK_ROOT
> $ANDROID_SDK_ROOT/cmdline-tools/tools/bin/sdkmanager --licenses
```

### Rust targets, NDK

After the Android SDK is installed, run the following script that will ll install the necessary Rust targets, the Android NDK and has built-in and helpful messages to ensure you will be able to compile for Android properly:

```
./libs/verify-android-environment.sh
````

You may have to run the above script multiple times till it succeeds, once it does you can try building using:

```
./gradlew assembleDebug
```

## iOS development

### Extra dependencies

- Carthage
- swift-protobuf
- xcpretty.

The following command will ensure your environment is ready to build the project for iOS:

```
./libs/verify-ios-environment.sh
````

The Xcode project is located at `megazords/ios/MozillaAppServices.xcodeproj`.
