# Using locally-published components in Fenix

It's often important to test work-in-progress changes to this repo against a real-world
consumer project. The most reliable method of performing such testing is to publish your
components to a local Maven repository, and adjust the consuming project to install them
from there.

With support from the upstream project, it's possible to do this in a single step using
our auto-publishing workflow.

## Using the auto-publishing workflow

Some consumers (notably [Fenix](https://github.com/mozilla-mobile/fenix/)) have support for
automatically publishing and including a local development version of application-services
in their build. The workflow is:

1. Check out the consuming project.
1. Edit (or create) the file `local.properties` *in the consuming project* and tell it where to
   find your local checkout of application-services, by adding a line like:

   `autoPublish.application-services.dir=path/to/your/checkout/of/application-services`

1. Build the consuming project following its usual build procedure, e.g. via `./gradlew assembleDebug` or `./gradlew
   test`.

If all goes well, this should automatically build your checkout of application-servies, publish it
to a local maven repository, and configure the consuming project to install from there instead of
from our published releases.

## Using a manual workflow

Note: This is a bit tedious, and you should first try the auto-publishing workflow described
above. But if the auto-publishing workflow bitrots then it's important to know how to do it
by hand. Since most consuming apps get their copy of application-services via a dependency
on android-components, this procedure involves three separate repos:

1. Inside the `application-services` repository root:
    1. In [`.buildconfig-android.yml`](app-services-yaml), change
       `libraryVersion` to end in `-TESTING$N` <sup><a href="#note1">1</a></sup>,
       where `$N` is some number that you haven't used for this before.

       Example: `libraryVersion: 0.27.0-TESTING3`
    2. Check your `local.properties` file, and add `rust.targets=x86` if you're
       testing on the emulator, `rust.targets=arm` if you're testing on 32-bit
       arm (arm64 for 64-bit arm, etc). This will make the build that's done in
       the next step much faster.

    3. Run `./gradlew publishToMavenLocal`. This may take between 5 and 10 minutes.

2. Inside the `android-components` repository root:
    1. In [`.buildconfig.yml`](android-components-yaml), change
       `componentsVersion` to end in `-TESTING$N` <sup><a href="#note1">1</a></sup>,
       where `$N` is some number that you haven't used for this before.

       Example: `componentsVersion: 0.51.0-TESTING3`
    2. Inside [`buildSrc/src/main/java/Dependencies.kt`](android-components-deps),
       change `mozilla_appservices` to reference the `libraryVersion` you
       published in step 1 part 1.

       Example: `const val mozilla_appservices = "0.27.0-TESTING3"`

    3. Inside [`build.gradle`](android-components-build-gradle), add
       `mavenLocal()` inside `allprojects { repositories { <here> } }`.

    4. Inside the android-components `local.properties` file, ensure
       `autoPublish.application-services.dir` is *NOT* set.

    5. Run `./gradlew publishToMavenLocal`.

3. Inside the consuming project repository root:
    1. Inside [`build.gradle`](fenix-build-gradle-1), add
       `mavenLocal()` inside `allprojects { repositories { <here> } }`.

    2. Ensure that `local.properties` does not contain any configuration to
       related to auto-publishing the application-services repo.

    3. Inside [`buildSrc/src/main/java/Dependencies.kt`](fenix-deps), change the
       version numbers for android-components and/or application-services to
       match the new versions you defined above.

       Example: `const val mozilla_android_components = "0.51.0-TESTING3"`

       Example: `const val mozilla_appservices = "0.27.0-TESTING3"`

You should now be able to build and run the consuming application (assuming you could
do so before all this).

### Caveats

1. This assumes you have followed the [android/rust build setup](./setup-android-build-environment.md)
2. Make sure you're fully up to date in all repos, unless you know you need to
   not be.
3. This omits the steps if changes needed because, e.g. application-services
   made a breaking change to an API used in android-components. These should be
   understandable to fix, you usually should be able to find a PR with the fixes
   somewhere in the android-component's list of pending PRs (or, failing that, a
   description of what to do in the application-services changelog).
4. Ask in #sync if you get stuck.


## Adding support for the auto-publish workflow

If you had to use the manual workflow above and found it incredibly tedious, you might like to
try adding support for the auto-publish workflow to the consuming project! The details will differ
depending on the specifics of the project's build setup, but at a high level you will need to:

1. In your [settings.gradle](fenix-settings), locate (or add) the code for parsing the `local.properties` file,
   and add support for loading a directory path from the property `autoPublish.application-services.dir`.

   If this property is present, spawn a subprocess to run `./gradlew autoPublishForLocalDevelopment`
   in the specified directory. This automates step (1) of the manual workflow above, publishing your
   changes to application-services into a local maven repository under a unique version number.

1. In your [build.gradle]([fenix-build-gradle-1]), if the `autoPublish.application-services.dir` property
   is present, have each project apply the build script from `./build-scripts/substitute-local-appservices.gradle`
   in the specified directory.

   This automates steps (2) and (3) of the manual workflow above, using gradle's dependency substitution
   capabilities to override the verion requirements for application-services components. It may be necessary
   to experiment with the ordering of this relative to other build configuration steps, in order for the
   dependency substitution to work correctly.

   For a single-project build this would look something like:

   ```groovy
   if (gradle.hasProperty('localProperties.autoPublish.application-services.dir')) {
      ext.appServicesSrcDir = gradle."localProperties.autoPublish.application-services.dir"
      apply from: "${appServicesSrcDir}/build-scripts/substitute-local-appservices.gradle"
   }
   ```

   For a multi-project build it should be applied to all subprojects, like:

   ```groovy
   subprojects {
      if (gradle.hasProperty('localProperties.autoPublish.application-services.dir')) {
         ext.appServicesSrcDir = gradle."localProperties.autoPublish.application-services.dir"
         apply from: "${rootProject.projectDir}/${appServicesSrcDir}/build-scripts/substitute-local-appservices.gradle"
      }
   }
   ```

1. Confirm that the setup is working, by adding `autoPublish.application-services.dir` to your
   `local.properties` file and running `./gradlew dependencies` for the project.

   You should be able to see gradle checking the build status of the various application-services
   dependencies as part of its setup phase. When the command completes, it should print the resolved
   versions of all dependencies, and you should see that application-services components have a version
   number in the format `0.0.1-SNAPSHOT-{TIMESTAMP}`.

---

<b id="note1">[1]</b>: It doesn't have to start with `-TESTING`, it only needs
to have the format `-someidentifier`. `-SNAPSHOT$N` is also very common to use,
however without the numeric suffix, this has specific meaning to gradle, so we
avoid it.  Additionally, while the `$N` we have used in our running example has
matched (e.g. all of the identifiers ended in `-TESTING3`, this is not required,
so long as you match everything up correctly at the end. This can be tricky, so
I always try to use the same number).

[app-services-yaml]: https://github.com/mozilla/application-services/blob/594f4e3f6c190bc5a6732f64afc573c09020038a/.buildconfig-android.yml#L1
[android-components-yaml]: https://github.com/mozilla-mobile/android-components/blob/b98206cf8de818499bdc87c00de942a41f8aa2fb/.buildconfig.yml#L1
[android-components-deps]: https://github.com/mozilla-mobile/android-components/blob/b98206cf8de818499bdc87c00de942a41f8aa2fb/buildSrc/src/main/java/Dependencies.kt#L37
[android-components-build-gradle]: https://github.com/mozilla-mobile/android-components/blob/b98206cf8de818499bdc87c00de942a41f8aa2fb/build.gradle#L28
[fenix-build-gradle-1]: https://github.com/mozilla-mobile/fenix/blob/f897c2e295cd1b97d4024c7a9cb45dceb7a2fa89/build.gradle#L26
[fenix-deps]:
https://github.com/mozilla-mobile/fenix/blob/f897c2e295cd1b97d4024c7a9cb45dceb7a2fa89/buildSrc/src/main/java/Dependencies.kt#L28
[fenix-settings]: https://github.com/mozilla-mobile/fenix/blob/0d398f7d44f877a61cd243ee9fac587a9d5c0a1f/settings.gradle#L31
