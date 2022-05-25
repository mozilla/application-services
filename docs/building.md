# Building Application Services

When working on Application Services, it's important to set up your environment for building the Rust code and the Android or iOS code needed by the application.

## First time builds

Building for the first time is more complicated than a typical Rust project.
To build for an end-to-end experience that enables you to test changes in
client applications like **Firefox for Android (Fenix)** and **Firefox iOS**, there are a number of build
systems required for all the dependencies. The initial setup is likely to take
a number of hours to complete.


## Building the Rust Components

*Complete this section before moving to the android/iOS build instructions.*
1. Make sure you cloned the repository:
  ```shell
    $ git clone https://github.com/mozilla/application-services # (or use the ssh link)
    $ cd application-services
    $ git submodule update --init --recursive
  ```
2. Install Rust: install [via rustup](https://www.rust-lang.org/tools/install)
3. Install your system dependencies:

    #### Linux
    1. Install the system dependencies required for building NSS
        1. Install gyp: `apt install gyp` (required for NSS)
        1. Install ninja-build: `apt install ninja-build`
        1. Install python3 (at least 3.6): `apt install python3`
        1. Install zlib: `apt install zlib1g-dev`
    1. Install the system dependencies required for SQLcipher
        1. Install tcl: `apt install tclsh` (required for SQLcipher)
    #### MacOS
    1. Install Xcode: check the [ci config](../.circleci/config.yml) for the correct
    version.
    1. Install Xcode tools: `xcode-select --install`
    1. Install homebrew: [via homebrew](https://brew.sh/) (it's what we use for ci)
    1. Install the system dependencies required for building NSS
        1. Install ninja: `brew install ninja`
        1. Install gyp (via https://github.com/mogemimi/pomdog/wiki/How-to-Install-GYP)
    #### Windows
    *Install windows build tools*

    > Why [Windows Subsystem for Linux (WSL)](https://docs.microsoft.com/en-us/windows/wsl/about)?
    >
    > It's currently tricky to get some of these builds working on Windows, primarily due to our use of SQLcipher. By using WSL it is possible to get builds working, but still have them published to your "native" local maven cache so it's available for use by a "native" Android Studio.

    1. Install [WSL](https://docs.microsoft.com/en-us/windows/wsl/about) (recommended over native tooling)
    1. Install unzip: `sudo apt install unzip`
    1. Install python3: `sudo apt install python3` *Note: must be python 3.6 or later*
    1. Install system build tools: `sudo apt install build-essential`
    1. Install zlib: `sudo apt-get install zlib1g-dev`
    1. Install tcl: `sudo apt install tcl-dev`
4. Check dependencies and environment variables by running: `./libs/verify-desktop-environment.sh`
  > Note that this script might instruct you to set some environment variables, set those by adding them to your
  `.zshrc` or `.bashrc` so they are set by default on your terminal
6. Run cargo test: `cargo test`

Once you have successfully run `./libs/verify-desktop-environment.sh` and `cargo test` you can move to the [**Building for Fenix**](building.md#building-for-fenix) and [**Building for iOS**](building.md#building-for-firefox-ios) sections below to setup your local environment for testing with our client applications.

---

## Building for Fenix
The following instructions assume that you are building `application-services` for Fenix, and want to take advantage of the
[Fenix Auto-publication workflow for android-components and application-services](howtos/locally-published-components-in-fenix.md).

1. Install Android SDK, JAVA, NDK and set required env vars
   1. Clone the [Fenix](https://github.com/mozilla-mobile/fenix/) repository (**not** inside the Application Service repository).
   1. Clone the [android-components](https://github.com/mozilla-mobile/android-components/) repository (**not** inside the Application Service repository).
   1. Install [Java **11**](https://www.oracle.com/java/technologies/downloads/#java11) for your system
   1. Set `JAVA_HOME` to point to the JDK 11 installation directory.
   1. Download and install [Android Studio](https://developer.android.com/studio/#downloads).
   1. Set `ANDROID_SDK_ROOT` and `ANDROID_HOME` to the Android Studio sdk location and add it to your rc file (either `.zshrc` or `.bashrc` depending on the shell you use for your terminal).
   1. Configure the required versions of NDK
  `Configure menu > System Settings > Android SDK > SDK Tools > NDK > Show Package Details > NDK (Side by side)`
        - 21.4.7075529 (required by Fenix; note: a specific NDK version isn't configured, this maps to default [NDK version](https://developer.android.com/studio/projects/install-ndk#default-ndk-per-agp) for the [AGP version](https://github.com/mozilla-mobile/fenix/blob/main/buildSrc/src/main/java/Dependencies.kt#L11))
        - 21.3.6528147 (required by Application Services, [as configured](https://github.com/mozilla/application-services/blob/main/build.gradle#L30))
1. If you are on Windows using WSL - drop to the section below, [Windows setup
for Android (WSL)](building.md#windows-setup-for-android-via-wsl) before proceeding.
1. Check dependencies, environment variables
   1. Run `./libs/verify-android-environment.sh`
   2. Follow instructions and rerun until it is successful.


### Windows setup for Android (via WSL)

Note: For non-Ubuntu linux versions, it may be necessary to execute `$ANDROID_HOME/tools/bin/sdkmanager "build-tools;26.0.2" "platform-tools" "platforms;android-26" "tools"`. See also [this gist](https://gist.github.com/fdmnio/fd42caec2e5a7e93e12943376373b7d0) for additional information.

#### Configure Maven

Configure maven to use the native windows maven repository - then, when doing `./gradlew install` from WSL, it ends up in the Windows maven repo. This means we can do a number of things with Android Studio in "native" windows and have then work correctly with stuff we built in WSL.

1. Install maven: `sudo apt install maven`
1. Confirm existence of (or create) a `~/.m2` folder
1. In the `~/.m2` create a file called `settings.xml`
1. Add the content below replacing `{username}` with your username:
```xml
    <settings>
      <localRepository>/mnt/c/Users/{username}/.m2/repository</localRepository>
    </settings>
```
---

## Building for Firefox iOS

1. Install [xcpretty](https://github.com/xcpretty/xcpretty#installation): `gem install xcpretty`
1. Run `./libs/verify-ios-environment.sh` to check your setup and environment
variables.  
1. Make any corrections recommended by the script and re-run.
2. Next, run `./megazords/ios-rust/build-xcframework.sh` to build all the binaries needed to consume a-s in iOS

Once the script passes, you should be able to run the Xcode project.
> Note: The built Xcode project is located at `megazords/ios-rust/MozillaTestServices.xcodeproj`.

> Note: This is mainly for testing the rust components, the artifact generated in the above steps should be all you need for building application with application-services



### Locally building Firefox iOS against a local Application Services

Detailed steps to build Firefox iOS against a local application services can be found [this document](./howtos/locally-published-components-in-firefox-ios.md)
