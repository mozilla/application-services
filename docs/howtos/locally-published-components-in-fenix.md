# Using locally published components in Fenix

It's often important to test work-in-progress changes to Application Services components against a real-world
consumer project. The most reliable method of performing such testing is to publish your
components to a local Maven repository, and adjust the consuming project to install them
from there.

With support from the upstream project, it's possible to do this in a single step using
our auto-publishing workflow.

# Using the auto-publishing workflow

mozilla-central has support for automatically publishing and including a local development version of application-services in the build.
This is supported for most of the Android targets available in mozilla-central including Fenix - this
doc will focus on Fenix, but the same general process is used for all. The workflow is:

## Pre-requisites:
1. Ensure you have a regular [build of application-services working](../building.md).
1. Disable the gradle cache in mozilla-central - edit `./gradle.properties`, comment out `org.gradle.configuration-cache=true`
1. Ensure you have a regular build of Fenix from mozilla-central testable in Android Studio or an emulator.

## Setup 2 x `local.properties`

Edit (or create) the file `local.properties` in each of the repos:

### app-services

In the root of the app-services repo:

Please be sure you have read [our guide to building Fenix](../building.md#building-for-fenix) and successfully built
using the instructions there. In particular, this may lead you to adding `sdk.dir` and `ndk.dir` properties, and/or
set environment variables `ANDROID_SDK_ROOT` and `ANDROID_HOME`.

In addition to those instructions, you will need:

#### rust.targets

Both the auto-publishing and manual workflows can be sped up significantly by
using the `rust.targets` property which limits which architectures the Rust
code gets build against.  Adding a line like
`rust.targets=x86,linux-x86-64`.  The trick is knowing which targets to put in
that comma separated list:

  - Use `x86` for running the app on most emulators on Intel hardware (in rare cases, when you have a 64-bit emulator, you'll want `x86_64`).
  - Use `arm64` for emulators running on Apple Silicon Macs.
  - If you're running the `android-components` or `fenix` unit tests, then you'll need the architecture of your machine:
    - OSX running Intel chips: `darwin-x86-64`
    - OSX running M1 chips: `darwin-aarch64`
    - Linux: `linux-x86-64`

eg, on a Mac your `local.properties` file will have a single line, `rust.targets=darwin-aarch64,arm64`

### mozilla-central

`local.properties` can be in the root of the mozilla-central checkout,
or in the project specific directory (eg, `mobile/android/fenix`) and you tell it where to
find your local checkout of application-services by adding a line like:

`autoPublish.application-services.dir=path/to/your/checkout/of/application-services`

Note that the path can be absolute or relative from `local.properties`. For example, if `application-services`
and `mozilla-central` are at the same level, and you are using a `local.properties` in the root of mozilla-central,
the relative path would be `../application-services`

## Build and test your Fenix again.

After configuring as described above, build and test your Fenix again.

If all goes well, this should automatically build your checkout of `application-services`, publish it
to a local maven repository, and configure the consuming project to install it from there instead of
from our published releases.

# Other notes
### Using Windows/WSL

Good luck! This implies you are also building mozilla-central in a Windows/WSL environment;
please contribute docs if you got this working.

However, there's an excellent chance that you will need to execute
`./automation/publish_to_maven_local_if_modified.py` from your local `application-services` root.

### Caveats

1. This assumes you are able to build both Fenix and application-services directly before following any of these instructions.
2. Make sure you're fully up to date in all repos, unless you know you need to
   not be.
4. [Contact us](../README.md#contact-us) if you get stuck.
