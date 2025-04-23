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
        1. Install perl (needed to build openssl): `apt install perl`
        1. Install patch (to build the libs): `apt install patch`

    1. Install the system dependencies required for SQLcipher
        1. Install tcl: `apt install tclsh` (required for SQLcipher)

    1. Install the system dependencies required for bindgen
        1. Install libclang: `apt install libclang-dev`

    #### MacOS
    1. Install Xcode: check the [ci config](https://github.com/mozilla/application-services/blob/main/.circleci/config.yml) for the correct version.
    1. Install Xcode tools: `xcode-select --install`
    1. Install homebrew via its [installation instructions](https://brew.sh/) (it's what we use for ci).
    1. Install the system dependencies required for building NSS:
        1. Install ninja and python: `brew install ninja python`
        1. Make sure `which python3` maps to the freshly installed homebrew python.
            1. If it isn't, add the following to your bash/zsh profile and `source` the profile before continuing:
                ```shell
                alias python3=$(brew --prefix)/bin/python3
                ```
            1. Ensure `python` maps to the same Python version. You may have to
               create a symlink:
               ```shell
               PYPATH=$(which python3); ln -s $PYPATH `dirname $PYPATH`/python
               ```
        1. Install gyp:
            ```shell
            wget https://bootstrap.pypa.io/ez_setup.py -O - | python3 -
            git clone https://chromium.googlesource.com/external/gyp.git ~/tools/gyp
            cd ~/tools/gyp
            pip install .
            ```
            1. Add `~/tools/gyp` to your path:
               ```shell
               export PATH="~/tools/gyp:$PATH"
               ```
            1. If you have additional questions, consult [this guide](https://github.com/mogemimi/pomdog/wiki/How-to-Install-GYP).
        1. Make sure your homebrew python's bin folder is on your path by updating your bash/zsh profile with the following:
            ```shell
            export PATH="$PATH:$(brew --prefix)/opt/python@3.9/Frameworks/Python.framework/Versions/3.9/bin"
            ```
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

4. [Build the required NSS libraries](https://github.com/mozilla/application-services/blob/main/libs/README.md).
4. Check dependencies and environment variables by running: `./libs/verify-desktop-environment.sh`
  > Note that this script might instruct you to set some environment variables, set those by adding them to your
  `.zshrc` or `.bashrc` so they are set by default on your terminal. If it does so instruct you, you must
  run the command again after setting them so the libraries are built.
5. Run cargo test: `cargo test`

Once you have successfully run `./libs/verify-desktop-environment.sh` and `cargo test` you can move to the [**Building for Fenix**](building.md#building-for-fenix) and [**Building for iOS**](building.md#building-for-firefox-ios) sections below to setup your local environment for testing with our client applications.

---

## Building for Fenix
The following instructions assume that you are building `application-services` for Fenix, and want to take advantage of the
[Fenix Auto-publication workflow for android-components and application-services](howtos/locally-published-components-in-fenix.md).

1. Install Android SDK, JAVA, NDK and set required env vars
   1. Clone the [firefox-android](https://github.com/mozilla-mobile/firefox-android) repository (**not** inside the Application Service repository).
   1. Install [Java **17**](https://www.oracle.com/java/technologies/downloads/#java17) for your system
   1. Set `JAVA_HOME` to point to the JDK 17 installation directory.
   1. Download and install [Android Studio](https://developer.android.com/studio/#downloads).
   1. Set `ANDROID_SDK_ROOT` and `ANDROID_HOME` to the Android Studio sdk location and add it to your rc file (either `.zshrc` or `.bashrc` depending on the shell you use for your terminal).
   1. Configure the required versions of NDK
  `Configure menu > System Settings > Android SDK > SDK Tools > NDK > Show Package Details > NDK (Side by side)`
        - 28.1.13356709 (required by Application Services, [as configured](https://github.com/mozilla/application-services/blob/main/build.gradle#L33))
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
    - Make any corrections recommended by the script and re-run.
1. Run `./automation/run_ios_tests.sh` to build all the binaries and run tests using the local SPM setup.

    > Note: This is mainly for testing the rust components, the artifact generated in the above steps should be all you need for building application with application-services



### Locally building Firefox iOS against a local Application Services

Detailed steps to build Firefox iOS against a local application services can be found [this document](./howtos/locally-published-components-in-firefox-ios.md)
