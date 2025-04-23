# Building and using a locally-modified version of JNA

[Java Native Access](https://github.com/java-native-access/jna/) is an important dependency
for the Application Services components on Android, as it provides the low-level interface
from the JVM into the natively-compiled Rust code.

If you need to work with a locally-modified version of JNA (e.g. to investigate an apparent
JNA bug) then you may find these notes helpful.

---

The JNA docs do have an [Android Development Environment guide](https://github.com/java-native-access/jna/blob/master/www/AndroidDevelopmentEnvironment.md)
that is a good starting point, but the instructions did not work for me and appear a little out of date.
Here are the steps that worked for me:

* Modify your environment to specify `$NDK_PLATFORM`, and to ensure the Android NDK tools
  for each target platform are in your `$PATH`. On my Mac with Android Studio the
  config was as follows:
    ```
    export NDK_ROOT="$HOME/Library/Android/sdk/ndk/28.1.13356709"
    export NDK_PLATFORM="$NDK_ROOT/platforms/android-25"
    export PATH="$PATH:$NDK_ROOT/toolchains/llvm/prebuilt/darwin-x86_64/bin"
    export PATH="$PATH:$NDK_ROOT/toolchains/aarch64-linux-android-4.9/prebuilt/darwin-x86_64/bin"
    export PATH="$PATH:$NDK_ROOT/toolchains/arm-linux-androideabi-4.9/prebuilt/darwin-x86_64/bin"
    export PATH="$PATH:$NDK_ROOT/toolchains/x86-4.9/prebuilt/darwin-x86_64/bin"
    export PATH="$PATH:$NDK_ROOT/toolchains/x86_64-4.9/prebuilt/darwin-x86_64/bin"
    ```
  You will probably need to tweak the paths and version numbers based on your operating system and
  the details of how you installed the Android NDK.

* Install the `ant` build tool (using `brew install ant` worked for me).

* Checkout the [JNA source](https://github.com/java-native-access/jna) from Github. Try doing a basic
  build via `ant dist` and `ant test`. This won't build for Android but will test the rest of the tooling.

* Adjust `./native/Makefile` for compatibility with your Android NSK install. Here's what I had to do for mine:
    * Adjust the `$CC` variable to use clang instead of gcc: `CC=aarch64-linux-android21-clang`.
    * Adjust thd `$CCP` variable to use the version from your system: `CPP=cpp`.
    * Add `-landroid -llog` to the list of libraries to link against in `$LIBS`.

* Build the JNA native libraries for the target platforms of interest:
    * `ant -Dos.prefix=android-aarch64`
    * `ant -Dos.prefix=android-armv7`
    * `ant -Dos.prefix=android-x86`
    * `ant -Dos.prefix=android-x86-64`

* Package the newly-built native libraries into a JAR/AAR using `ant dist`.
  This should produce `./dist/jna.aar`.

* Configure `build.gradle` for the consuming application to use the locally-built JNA artifact:
    ```
    // Tell gradle where to look for local artifacts.
    repositories {
        flatDir {
            dirs "/PATH/TO/YOUR/CHECKOUT/OF/jna/dist"
        }
    }

    // Tell gradle to exclude the published version of JNA.
    configurations {
        implementation {
            exclude group: "net.java.dev.jna", module:"jna"
        }
    }

    // Take a direct dependency on the local JNA AAR.
    dependencies {
        implementation name: "jna", ext: "aar"
    }
    ```

* Rebuild and run your consuming application, and it should be using the locally-built JNA!

If you're trying to debug some unexpected JNA behaviour (and if you favour old-school printf-style debugging)
then you can this code snippet to print to the Android log from the compiled native code:

```
#ifdef __ANDROID__
#include <android/log.h>
#define HACKY_ANDROID_LOG(...) __android_log_print(ANDROID_LOG_VERBOSE, "HACKY-DEBUGGING-FOR-ALL", __VA_ARGS__)
#else
#define HACKY_ANDROID_LOG(MSG)
#endif

HACKY_ANDROID_LOG("this will go to the android logcat output");
HACKY_ANDROID_LOG("it accepts printf-style format sequences, like this: %d", 42);
```
