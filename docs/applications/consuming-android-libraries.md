---
id: consuming-android-libraries
title: Consuming Android libraries
sidebar_label: Consuming Android libraries
---

# Consuming Android libraries

## Megazord (composite) libraries

The Rust component libraries that Application Services publishes stand alone: each published Android
ARchive (AAR) contains managed code (`classes.jar`) and multiple `.so` library files (one for each
supported architecture).  That means consuming multiple such libraries entails at least two `.so`
libraries, and each of those libraries includes the entire Rust standard library as well as
(potentially many) duplicated dependencies.  To save space and allow cross-component native-code
Link Time Optimization (LTO, i.e., inlining, dead code elimination, etc) Application Services also
publishes composite libraries -- so called *megazord libraries* -- that compose multiple Rust
components into a single optimized `.so` library file.  The managed code can be easily configured to
use such a megazord without significant changes.

There are two tasks we want to arrange.  First, we want to substitute component modules for the single
megazord module; second, we want to arrange for native Rust code to be available to JVM unit tests.

Both tasks are handled by the [megazord-gradle](https://github.com/mozilla/megazord-gradle) Gradle
plugin; see that page for details on how to apply the plugin to your build configuration.

## Consuming megazords

To consume a specific megazord module, use something like:

```groovy
appservices {
    defaultConfig {
        // Megazord in all variants.  The default is to not megazord.
        megazord = 'org.mozilla.appservices.megazord'
        // With no configuration, the default is to enable unit tests for all variants.
        enableUnitTests = true
    }

    // or:

    buildTypes {
        debug {
            megazord = 'org.mozilla.appservices.megazord'
            enableUnitTests = true
        }
        // Do not megazord or enable unit tests in release build type.
    }

    // or:

    productFlavors {
        stage {
            megazord = 'org.mozilla.appservices.megazord'
            enableUnitTests = true
        }
        // Do not megazord or enable unit tests in other product flavors.
    }

    // or:

    variants {
        stageDebug {
            megazord = 'org.mozilla.appservices.megazord'
            enableUnitTests = true
        }
        // Do not megazord or enable unit tests in other variants.
    }
}
```

### Configuring the consuming application

The megazord modules expose a single additional JVM class, like
`org.mozilla.appservices.composites.{Lockbox,ReferenceBrowser}Composition`.  That class has a single
static `init()` method that consuming applications should invoke in their `Application.onCreate`,
like:

```xml
<manifest>
    <application android:name=".Application" ...>
    </application>
    ...
</manifest>
```

and:

```java
public class Application extends android.app.Application {
    @Override
    public void onCreate() {
        super.onCreate();

        LockboxComposition.init();
    }

    ...
}
```

This `init()` method sets some Java system properties that tell the component modules what native
code library contains the underlying component native code.

### Pseudo-code details

For each Android variant `variant`, the corresponding Gradle configurations
(`variant.{compileConfiguration,runtimeConfiguration}`) have module substitutions applied, like:

```groovy
// Pseudo-code!
configuration.resolutionStrategy.dependencySubstitution.all { dependency ->
    if (dependency.isComponentModule()) {
        dependency.useTarget('org.mozilla.appservices.megazord:megazord:...')
    }
}
```

## Unit testing Rust native code

The Application Services Maven publications contain Rust native code targeting Android devices.  To
unit test against the provided functionality, we require an additional dependency that packages Rust
native code for use on Desktop hosts and Java Native Access (JNA) internals in a form suitable for
consuming in Robolectric unit tests.

### Pseudo-code details

For unit testing support, for each test variant `variant.unitTestVariant`, the corresponding Gradle
configurations (`variant.unitTestVariant.{compileConfiguration,runtimeConfiguration}`) have
additional dependencies added, like:

```groovy
// Pseudo-code!
dependencies {
    configuration.name 'org.mozilla.places-forUnitTests:places-forUnitTests:...'
}
```

When a megazord is used, the additional dependencies will be megazord-specific, like:


```groovy
// Pseudo-code!
dependencies {
    configuration.name 'org.mozilla.appservices.megazord-forUnitTests:meg-forUnitTests:...'
}
```

## Megazord Maven details

The megazord Maven publication is a shell Android ARchive (AAR) that contains a native library and
depends on special `-withoutLibs` versions of the component modules.  For example, we have:

```
org.mozilla.sync15/logins.aar
- classes.jar
- libs/liblogins_ffi.so
```

and a `-withoutLibs` version, like:

```
org.mozilla.sync15-withoutLibs/logins-withoutLibs.aar
- classes.jar
```

and then a megazord like:

```
org.mozilla.appservices.composites/lockbox.aar
- libs/liblockbox.so
```

The `org.mozilla.appservices.composite:lockbox` Maven publication then depends on
`org.mozilla.sync15-withoutLibs:logins-withoutLibs` so that the JVM code (`classes.jar`) is used but
the component module native library (`libs/liblogins_ffi.so`) is not.

## Application Services Maven repository details

The megazord libraries and dependencies aren't yet published to maven.mozilla.org (see
[issue #252](https://github.com/mozilla/application-services/issues/252)) and for technical reasons
they aren't yet mirrored to jcenter either (see
[this bintray plugin issue](https://github.com/bintray/gradle-bintray-plugin/issues/130)).

That means we need a [non-standard Maven repository](https://bintray.com/ncalexander/application-services):
```groovy
repositories {
    maven {
        name 'nalexander\'s personal bintray'
        url 'https://dl.bintray.com/ncalexander/application-services'
    }
}
```

The [megazord-gradle](https://github.com/mozilla/megazord-gradle) Gradle plugin adds needed
repositories automatically.
