## Doing a local build of the Android Components:

This document describes how to make local builds of the Android components in
this repository. Most consumers of these components *do not* need to follow
this process, but will instead use pre-built components published by the
[android-components](https://github.com/mozilla-mobile/android-components/) project.

This document, and the build process itself, is a work-in-progress - please file issues
(or even better, pull requests!) if you notice errors or omissions.

## Prepare your build environment

*NOTE: This section is almost certainly incomplete - given it is typically
only done once, things have probably been forgotten or over-simplified.
Please file PRs if you notice errors or omissions here*

This process should work OK on Mac and Linux. It also works on [Windows via WSL by following these instructions](#using-windows).

Typically, this process only needs to be run once, although periodically you
may need to repeat some steps (eg, rust updates should be done periodically)

1. Prepare the [Android SDK and NDK](#setting-up-the-android-sdk-and-ndk)

2. Install `rustup` from https://rustup.rs:
    - If you already have it, run `rustup update`
    - Run `rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`

3. Ensure your clone of `mozilla/application-services` is up to date.

4. Build NSS and SQLCipher
    - `cd path/to/application-services/libs` (Same dir you were just in for step 4)
    - `./build-all.sh android` (Go make some coffee or something, this will take
       some time as it has to compile NSS and SQLCipher for x86, x86_64, arm, and arm64).
    - Note that if something goes wrong here
        - Execute `libs/verify-android-environment.sh`
        - Double-check everything mentioned here.
        - Remove the `libs/android` folder and try again.
        - Turn it off and on again? ;)

## Building

Having done the above, the build process is the easy part! Note however that you
probably just want to use the `autoPublish` steps below.

1. Ensure your clone of application-services is up-to-date.

2. Ensure rust is up-to-date by running `rustup`

3. The builds are all performed by `./gradlew` and the general syntax used is
   `./gradlew project:task`

   You can see a list of projects by executing `./gradlew projects` and a list
   of tasks by executing `./gradlew tasks`.

### Using `autoPublish` features in android-components and fenix.

Both Fenix and android-components support `autopublish` via maven. In short,
edit `local.properties` in the project of interest and add:

`autoPublish.application-services.dir={path-to-local-application-services-checkout}`

then just do a build! Magic happens - eg, from the root of the Fenix dir,

`./gradlew assembleDebug` builds the world.

See (this document for even more details)[locally-published-components-in-fenix.md]

### Other build types

If you just want the build artifacts, you probably want one of the `assemble` tasks - either
   `assembleDebug` or `assembleRelease`.

For example, to build a debug version of the places library, the command you
want is `./gradlew places:assembleDebug`

After building, you should find the built artifact under the `target` directory,
with sub-directories for each Android architecture. For example, after executing:

    % ./gradlew places:assembleDebug

you will find:

    target/aarch64-linux-android/release/libplaces_ffi.so
    target/x86_64-linux-android/release/libplaces_ffi.so
    target/i686-linux-android/release/libplaces_ffi.so
    target/armv7-linux-androideabi/release/libplaces_ffi.so

(You will probably notice that even though you used `assembleDebug`, the directory names are `release` - this may change in the future)

You should also find the .kt files for the project somewhere there and in the right directory structure if that turns out to be useful.

# Setting up the Android SDK and NDK.

These instructions have recently worked. Older instructions that recently failed
are below for reference.

1. Install or locate the Android SDKs
   - Install the Android SDKs. If Android Studio is involved, it may have already installed
     them somewhere - use the "SDK Manager" to identify this location.
   - Set `ANDROID_HOME` to this location and add it to your rc file.

  If not already installed, you can install the "command-line" only tools:

   - Visit https://developer.android.com/studio/, at the bottom of the page locate the current SDKs for linux
     at time of writing, this is https://dl.google.com/android/repository/commandlinetools-linux-6514223_latest.zip

  Install it by executing:

  > % cd ~  
  > % mkdir -p android-sdk/cmdline-tools  
  > % cd android-sdk/cmdline-tools  
  > % unzip {path-to.zip}  
  > % export ANDROID_HOME=$HOME/android-sdk  
  > % $ANDROID_HOME/cmdline-tools/tools/bin/sdkmanager "platforms;android-26"  
  > % $ANDROID_HOME/cmdline-tools/tools/bin/sdkmanager --licenses  

  Note that the The `cmdline-tools/tools` structure appears necessary for somewhat opaque reasons, see [stack overflow, obviously](https://stackoverflow.com/questions/60440509/android-command-line-tools-sdkmanager-always-shows-warning-could-not-create-se) for more.

2. Install the Android NDK

   - Install the NDK directly via the SDK manager. To work out exactly what
     version you need, try executing `./gradlew assembleDebug`, for example, and
     the build should fail telling to exactly what's missing - eg:

  > Compatible side by side NDK version was not found. Default is 20.0.5594570.

   - If you have the command-line tools:

  > `$ANDROID_HOME/cmdline-tools/tools/bin/sdkmanager --install "ndk;20.0.5594570"

   - If you have Android Studio or otherwise a GUI version of the SDK, use that
     GUI.

  In either case, you don't need to select where it is installed or to set
  further environment variables.

At the end of this process, all you need is `ANDROID_HOME` set!

## Older instructions for the Android SDK and NDK.

The more recent tools have simplified much of the process - however, at time of
writing, this has only been tested on Ubuntu via WSL - so we're keeping this
around until we get wider confirmation that it's out-dated.

At the end of this process you should have the following environment variables set up.

- `ANDROID_NDK_ROOT`
- `ANDROID_NDK_HOME`
- `ANDROID_NDK_API_VERSION`
- `ANDROID_HOME`
- `JAVA_HOME`

These variables are required every time you build, so you should add them to
a rc file or similar so they persist between reboots etc.

1. Install the SDK, largely as described above.

2. Install NDK r21 from https://developer.android.com/ndk/downloads
    - Extract it, put it somewhere (`$HOME/.android-ndk-r21` is a reasonable
      choice, but it doesn't matter), and set `ANDROID_NDK_ROOT` to this location.
    - Set `ANDROID_NDK_HOME` to match `ANDROID_NDK_ROOT`, for compatibility with
      some android grandle plugins.

    (Note with the most recent SDK, this step works, but the build fails with:
> Configure project :full-megazord
> WARNING: Compatible side by side NDK version was not found. Default is 20.0.5594570.
    ), hence the instructions above re installing the SDK directly via the SDK
    manager.

3. Install or locate Java
    - Check if java is already installed - if it is, you probably don't need to
      do anything. Eg, on a standard Ubuntu 18:

    $ java --version
    openjdk 11.0.7 2020-04-14
    OpenJDK Runtime Environment (build 11.0.7+10-post-Ubuntu-2ubuntu218.04)
    OpenJDK 64-Bit Server VM (build 11.0.7+10-post-Ubuntu-2ubuntu218.04, mixed mode, sharing)

    There's no `JAVA_HOME` set, but that's OK - it's ready to go.

    Otherwise:

    - Either install Java, or, if Android Studio is installed, you can probably find one
      installed in a `jre` directory under the Android Studio directory.
    - Set `JAVA_HOME` to this location and add it to your rc file.


# Using Windows

It's currently tricky to get some of these builds working on Windows, primarily due to our use of `sqlcipher`. However, by using the Windows Subsystem for Linux, it is possible to get builds working, but still have them published to your "native" local maven cache so it's available for use by a "native" Android Studio.

As above, this document may be incomplete, so please edit or open PRs where necessary.

In general, you will follow the exact same process outlined above, with one or 2 unique twists.

## Setting up the build environment

You need to install most of the build tools in WSL. This means you end up with many tools installed twice - once in WSL and once in "native" Windows - but the only cost of that is disk-space.

You will need the following tools in WSL:

* unzip - `sudo apt install unzip`

* python 3 - `sudo apt install python3` (XXX - TODO - you actually need 3.6
  and if your distro doesn't some with that, you need to google how to make it
  work with both the installed version and the updated version. Good luck!)

* Build tools (gcc, etc) - `sudo apt install build-essential`

* zlib support - `sudo apt-get install zlib1g-dev`

* java - you probably already have it? Ubuntu 18 does and nothing special needs
  to be done. If you are using earlier Ubuntu versions you are in for a world of
  pain, so just update.

* tcl, used for sqlcipher builds - `sudo apt install tcl-dev`

Notes:

* It may be necessary to execute `$ANDROID_HOME/tools/bin/sdkmanager "build-tools;26.0.2" "platform-tools" "platforms;android-26" "tools"`,
  but may not - it didn't seem to be necessary on recent Ubuntus. See also 
  [this gist](https://gist.github.com/fdmnio/fd42caec2e5a7e93e12943376373b7d0)
  which google found for me and might have useful info.

## Configure Maven

We now want to configure maven to use the native windows maven repository - then,
when doing `./gradlew install` from WSL, it ends up in the Windows maven repo.
This means we can do a number of things with Android Studio in "native" windows
and have then work correctly with stuff we built in WSL.

* Execute `sudo apt install maven` - this should have created a `~/.m2` folder as the WSL maven repository (if it didn't, just make it yourself). In this directory, create a file `~/.m2/settings.xml` with the content:

    ```
    <settings>
      <localRepository>/mnt/c/Users/{username}/.m2/repository</localRepository>
    </settings>
    ```

  (obviously with {username} adjusted appropriately)

* Now you should be ready to roll - `./gradlew install` should complete and publish the components to your native maven repo!

\o/

# Other obsolete content

This content is probably outdated. It should either be removed, or saved!

## Publishing to your local maven repo

Note that this has been make obsolete by the `autoPublish` mechanisms above.

The easiest way to use the build is to have your Android project reference the component from your local maven repository - this is done by the `publishToMavenLocal` task - so:

    ./gradlew publishToMavenLocal

should work. Check your `~/.m2` directory (which is your local maven repo) for the components.

You can also publish single projects - eg:

    ./gradlew service-sync-places:publishToMavenLocal

For more information about using the local maven repo, see this [android components guide](https://mozilla-mobile.github.io/android-components/contributing/testing-components-inside-app).
