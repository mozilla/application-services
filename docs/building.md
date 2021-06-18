# Building Application Services

When working on application-services Rust Components it's important to set up your environment for both building the application-services library, but also for Android or iOS as appropriate to test your changes in our clients.

## First time builds

Building for the first time is more complicated than a typical Rust project.
To build for an end-to-end experience that enables you to test changes in our
client applications like Fenix and Firefox iOS, there are a number of build
systems required for all the dependencies. The initial setup is likely to take
a number of hours to complete.


## Building the Rust Components

*Complete this section before moving to the android/iOS build instructions.*
1. Make sure you cloned the repository:
  ```shell
    $ git clone https://github.com/mozilla/application-services # (or use the ssh link)
    $ cd application-services
    $ git submodule init
    $ git submodule update --recursive
  ```
2. Install Rust: install [via rustup](https://www.rust-lang.org/tools/install)
3. Install your system dependencies: - install via the instructions below for [Linux](building.md#linux), [MacOS](building.md#macos) or [Windows](building.md#windows)
4. Check dependencies, environment variables and test
   1. Run: `./libs/verify-desktop-environment.sh`
   1. Run: `cargo test`

Once you have successfully run `./libs/verify-desktop-environment.sh` and `cargo test` you can move to the **Building for Fenix** and **Building for iOS** sections below to setup your local environment for testing with our client applications.


#### Linux
1. Install the system dependencies required for building NSS
  1. Install gyp: `apt install gyp` (required for NSS)
  1. Install ninja-build: [via package for distribution](https://github.com/ninja-build/ninja/wiki/Pre-built-Ninja-packages#package-managers)
  1. Install python3: [3.6 via python.org](https://docs.python.org/3/using/unix.html)
  1. Install zlib: `apt install zlib1g-dev`
1. Install the system dependencies required for SQLcipher
  1. Install tcl: `apt install tclsh` (required for SQLcipher)


#### MacOS
1. Install Xcode: check the [ci config](../.circleci/config.yml) for the correct
version.
1. Install Xcode tools: `xcode-select --install`
1. Install homebrew: [via homebrew](https://brew.sh/) (its what we use for ci)
1. Install the system dependencies required for building NSS
    1. Install ninja: `brew install ninja`
    1. Install gyp (via https://github.com/mogemimi/pomdog/wiki/How-to-Install-GYP)
1. Install swift-protobuf: `brew install swift-protobuf`


#### Windows
*Install windows build tools*

> Why [Windows Subsystem for Linux (WSL)](https://docs.microsoft.com/en-us/windows/wsl/about)?
>
> It's currently tricky to get some of these builds working on Windows, primarily due to our use of SQLcipher. By using WSL it is possible to get builds working, but still have them published to your "native" local maven cache so it's available for use by a "native" Android Studio.

1. Install [WSL](https://docs.microsoft.com/en-us/windows/wsl/about) (recommended over native tooling)
1. Install unzip: `sudo apt install unzip`
1. Install python3: `sudo apt install python3` *Note: must be python 3.6*
1. Install system build tools: `sudo apt install build-essential`
1. Install zlib: `sudo apt-get install zlib1g-dev`
1. Install tcl: `sudo apt install tcl-dev`

---

## Building for Fenix
The instructions here assume that you are building for Fenix in order test your changes in Fenix and want to take advantage of the
[Fenix Auto-publication workflow for android-components and application-services](https://github.com/mozilla-mobile/fenix/#auto-publication-workflow-for-android-components-and-application-services)

1. Install Android SDK, JAVA, NDK and set required env vars
   1. Clone the [Fenix](https://github.com/mozilla-mobile/fenix/) repository (not in a-s)
   1. Clone the [android-components](https://github.com/mozilla-mobile/android-components/) repository (not in a-s)
   1. Install [Java **8**] for your system
   1. Set `JAVA_HOME` to point to the JDK 8 installation directory.
   1. Download and install [Android Studio](https://developer.android.com/studio/#downloads)
   1. Set `ANDROID_SDK_ROOT` and `ANDROID_HOME` to the Android Studio sdk location and add it to your rc file.
   1. Configure the required versions of NDK
  `Configure menu > System Settings > Android SDK > SDK Tools > NDK > Show Package Details > NDK (Side by side)`
        - 21.3.6528147 (required by Fenix)
        - 21.0.6113669 (required by a-s)
1. If you are on Windows using WSL - drop to the section below, Windows setup
for Android (WSL) before proceeding.
1. Check dependencies, environment variables and test
   1. Run `./libs/verify-android-environment.sh`
   2. Follow instructions and rerun until it is successful.


### Windows setup for Android (via WSL)

Note: For non-Ubuntu linux versions, it may be necessary to execute `$ANDROID_HOME/tools/bin/sdkmanager "build-tools;26.0.2" "platform-tools" "platforms;android-26" "tools"`. See also [this gist](https://gist.github.com/fdmnio/fd42caec2e5a7e93e12943376373b7d0) for additional info.

#### Configure Maven

Configure maven to use the native windows maven repository - then, when doing ./gradlew install from WSL, it ends up in the Windows maven repo. This means we can do a number of things with Android Studio in "native" windows and have then work correctly with stuff we built in WSL.

1. Install maven: `sudo apt install maven`
1. Confirm existence of (or create) a `~/.m2` folder
1. In the `~/.m2` create a file called `settings.xml`
1. Add the content below replacing `{username}` with your username:
```
    <settings>
      <localRepository>/mnt/c/Users/{username}/.m2/repository</localRepository>
    </settings>
```
---

## Building for Firefox iOS

1. Install Carthage: `brew install carthage`
1. Install [xcpretty](https://github.com/xcpretty/xcpretty#installation): `gem install xcpretty`
1. Run `./libs/verify-ios-environment.sh` to check your setup and environment
variables.  
1. Make any corrections recommended by the script and re-run.
1. Follow the guide for [using local a-s builds in iOS](https://github.com/mozilla/application-services/blob/main/docs/howtos/locally-published-components-in-ios.md#using-locally-published-components-in-firefox-for-ios)

> Note: The built Xcode project is located at `megazords/ios/MozillaAppServices.xcodeproj`.
