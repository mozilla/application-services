# Using locally published components in Fenix

It's often important to test work-in-progress changes to Application Services components against a real-world
consumer project. The most reliable method of performing such testing is to publish your
components to a local Maven repository, and adjust the consuming project to install them
from there.

With support from the upstream project, it's possible to do this in a single step using
our auto-publishing workflow.

## rust.targets

Both the auto-publishing and manual workflows can be sped up significantly by
using the `rust.targets` property which limits which architectures the Rust
code gets build against.  You can set this property by creating/editing the
`local.properties` file in the repository root and adding a line like
`rust.targets=x86,linux-x86-64`.  The trick is knowing which targets to put in
that comma separated list:

  - Use `x86` for running the app on most emulators (in rare cases, when you have a 64-bit emulator, you'll want `x86_64`)
  - If you're running the `android-components` or `fenix` unit tests, then you'll need the architecture of your machine:
    - OSX running Intel chips: `darwin-x86-64`
    - OSX running M1 chips: `darwin-aarch64`
    - Linux: `linux-x86-64`

## Using the auto-publishing workflow

mozilla-central has support for automatically publishing and including a local development version of application-services in the build.
This is supported for most of the Android targets available in mozilla-central including Fenix - this
doc will focus on Fenix, but the same general process is used for all. The workflow is:

1. Ensure you have a regular build of Fenix working from mozilla-central and that you've done a `./mach build`
1. Ensure you have a regular [build of application-services working](../building.md).
1. Edit (or create) the file `local.properties` - this can be in the root of the mozilla-central checkout,
   or in the project specific directory (eg, `mobile/android/fenix`) and tell it where to
   find your local checkout of application-services, by adding a line like:

   `autoPublish.application-services.dir=path/to/your/checkout/of/application-services`

   Note that the path can be absolute or relative from `local.properties`. For example, if `application-services`
   and `mozilla-central` are at the same level, and you are using a `local.properties` in the root of mozilla-central,
   the relative path would be `../application-services`
1. Build your target normally - eg, in Android Studio. or using `gradle`

If all goes well, this should automatically build your checkout of `application-services`, publish it
to a local maven repository, and configure the consuming project to install it from there instead of
from our published releases.

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
