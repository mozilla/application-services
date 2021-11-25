# Using locally published components in Fenix

It's often important to test work-in-progress changes to Application Services components against a real-world
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

   `autoPublish.application-services.dir=relative/path/to/your/checkout/of/application-services`

   Note that the path should be relative from the root of the consumer's directory. For example, if `application-services`
   and `fenix` are at the same level, the relative path would be `../application-services`
1. Build the consuming project following its usual build procedure, e.g. via `./gradlew assembleDebug` or `./gradlew
   test`.

If all goes well, this should automatically build your checkout of `application-services`, publish it
to a local maven repository, and configure the consuming project to install it from there instead of
from our published releases.

## Using a manual workflow

Note: This is a bit tedious, and you should first try the auto-publishing workflow described
above. But if the auto-publishing workflow fails then it's important to know how to do the publishing process manually. Since most consuming apps get their copy of `application-services` via a dependency
on `android-components`, this procedure involves three separate repos:

1. Inside the `application-services` repository root:
    1. In [`.buildconfig-android.yml`](https://github.com/mozilla/application-services/blob/main/.buildconfig-android.yml), change
       `libraryVersion` to end in `-TESTING$N` <sup><a href="#note1">1</a></sup>,
       where `$N` is some number that you haven't used for this before.

       Example: `libraryVersion: 0.27.0-TESTING3`
    2. Check your `local.properties` file, and add `rust.targets=x86` if you're
       testing on the emulator, `rust.targets=arm` if you're testing on 32-bit
       arm (arm64 for 64-bit arm, etc). This will make the build that's done in
       the next step much faster.

    3. Run `./gradlew publishToMavenLocal`. This may take between 5 and 10 minutes.

2. Inside the `android-components` repository root:
    1. In [`.buildconfig.yml`](https://github.com/mozilla-mobile/android-components/blob/main/.buildconfig.yml), change
       `componentsVersion` to end in `-TESTING$N` <sup><a href="#note1">1</a></sup>,
       where `$N` is some number that you haven't used for this before.

       Example: `componentsVersion: 0.51.0-TESTING3`
    2. Inside [`buildSrc/src/main/java/Dependencies.kt`](https://github.com/mozilla-mobile/android-components/blob/main/buildSrc/src/main/java/Dependencies.kt),
       change `mozilla_appservices` to reference the `libraryVersion` you
       published in step 1 part 1.

       Example: `const val mozilla_appservices = "0.27.0-TESTING3"`

    3. Inside [`build.gradle`](https://github.com/mozilla-mobile/android-components/blob/main/build.gradle), add
       `mavenLocal()` inside `allprojects { repositories { <here> } }`.

    4. Inside the android-components `local.properties` file, ensure
       `autoPublish.application-services.dir` is *NOT* set.

    5. Run `./gradlew publishToMavenLocal`.

3. Inside the consuming project repository root:
    1. Inside [`build.gradle`](https://github.com/mozilla-mobile/fenix/blob/main/build.gradle), add
       `mavenLocal()` inside `allprojects { repositories { <here> } }`.

    2. Ensure that `local.properties` does not contain any configuration to
       related to auto-publishing the application-services repo.

    3. Inside [`buildSrc/src/main/java/AndroidComponents.kt`](https://github.com/mozilla-mobile/fenix/blob/main/buildSrc/src/main/java/AndroidComponents.kt), change the
       version numbers for android-components to
       match the new versions you defined above.

       Example: `const val VERSION = "0.51.0-TESTING3"`

You should now be able to build and run the consuming application (assuming you could
do so before all this).

### Caveats

1. This assumes you have followed the [build instructions for Fenix](../building.md#building-for-fenix)
2. Make sure you're fully up to date in all repos, unless you know you need to
   not be.
3. This omits the steps if changes needed because, e.g. `application-services`
   made a breaking change to an API used in `android-components`. These should be
   understandable to fix, you usually should be able to find a PR with the fixes
   somewhere in the android-component's list of pending PRs (or, failing that, a
   description of what to do in the application-services changelog).
4. [Contact us](../README.md#contact-us) if you get stuck.


## Adding support for the auto-publish workflow

If you had to use the manual workflow above and found it incredibly tedious, you might like to
try adding support for the auto-publish workflow to the consuming project! The details will differ
depending on the specifics of the project's build setup, but at a high level you will need to:

1. In your [settings.gradle](https://github.com/mozilla-mobile/fenix/blob/main/settings.gradle), locate (or add) the code for parsing the `local.properties` file,
   and add support for loading a directory path from the property `autoPublish.application-services.dir`.

   If this property is present, spawn a subprocess to run `./gradlew autoPublishForLocalDevelopment`
   in the specified directory. This automates step (1) of the manual workflow above, publishing your
   changes to `application-services` into a local maven repository under a unique version number.

1. In your [build.gradle](https://github.com/mozilla-mobile/fenix/blob/main/build.gradle), if the `autoPublish.application-services.dir` property
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
