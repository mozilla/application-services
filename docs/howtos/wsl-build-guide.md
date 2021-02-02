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
