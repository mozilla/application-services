# Mozilla Application Services Gradle Plugin

Plugin for consuming Mozilla Application Services megazord native libraries.

<p align="left">
    <a alt="Version badge" href="https://plugins.gradle.org/plugin/org.mozilla.appservices.gradle-plugin">
        <img src="https://img.shields.io/maven-metadata/v/https/plugins.gradle.org/m2/org/mozilla/appservices/org.mozilla.appservices.gradle.plugin/maven-metadata.xml.svg?label=org.mozilla.appservices&colorB=brightgreen" /></a>
</p>

## Overview

Mozilla Application Services publishes many native (Rust) code libraries that stand alone: each
published Android ARchive (AAR) contains managed code (`classes.jar`) and multiple `.so` library
files (one for each supported architecture).  That means consuming multiple such libraries entails
at least two `.so` libraries, and each of those libraries includes the entire Rust standard library
as well as (potentially many) duplicated dependencies.  To save space and allow cross-component
native-code Link Time Optimization (LTO, i.e., inlining, dead code elimination, etc) Application
Services also publishes composite libraries -- so called *megazord libraries* or just *megazords* --
that compose multiple Rust components into a single optimized `.so` library file.  The managed code
can be easily configured to use such a megazord without additional changes.

The `org.mozilla.appservices` plugin makes it easy to consume such megazord libraries.

## Configuration

```groovy
appservices {
    defaultConfig {
        // Megazord in all Android variants.  The default is to not megazord.
        megazord = 'lockbox' // Or 'reference-browser', etc.
        enableUnitTests = false // Defaults to true.
    }
}
```

You can configure per Android variant, per Android product flavor, or per Android build
type (in order of preference, i.e., a matching variant is preferred to a matching product flavor is
preferred to a matching build type is preferred to the default config).

```groovy
appservices {
    variants {
        stageDebug {
            megazord = 'org.mozilla.appservices.megazord'
            enableUnitTests = false // Defaults to true.
        }
        // Do not megazord or enable unit tests in other variants.
    }

    // overrides:

    productFlavors {
        stage {
            megazord = 'org.mozilla.appservices.megazord'
        }
        // Do not megazord or enable unit tests in other product flavors.
    }

    // overrides:

    buildTypes {
        debug {
            megazord = 'org.mozilla.appservices.megazord'
        }
        // Do not megazord or enable unit tests in release build type.
    }

    // overrides defaultConfig.
}
```

### `megazords`

New megazord definitions can be defined, and existing megazord definitions modified, using the
`megazords` block.  For example, the existing "lockbox" megazord could be defined like:

```groovy
appservices {
    megazords {
        lockbox {
            moduleIdentifier 'org.mozilla.appservices:lockbox-megazord'
            component 'org.mozilla.appservices', 'fxaclient'
            component 'org.mozilla.appservices', 'logins'
        }
    }
}
```

while the existing "reference-browser" megazord could be modified to match the "lockbox" megazord
like:

```groovy
appservices {
    megazords {
        "reference-browser" {
            moduleIdentifier 'org.mozilla.appservices:reference-browser-megazord'
            components.clear()
            component 'org.mozilla.appservices', 'fxaclient'
            component 'org.mozilla.appservices', 'logins'
        }
    }
}
```

To reset the known Mozilla megazords:

```groovy
appservices {
   // Reset to the default megazord definitions.
   setMozillaMegazords()
}
```

## Development

To run the integration tests:

```
./gradlew -p gradle-plugin test
```

To publish locally, for testing against a consuming application:

```
./gradlew -p gradle-plugin publishToMavenLocal
```

Then use `mavenLocal()` in the consuming application to find the updated version.

To publish to the Gradle plugin portal (requires credentials in `$HOME/.gradle/gradle.properties`):

```
./gradlew -p gradle-plugin publishPlugin
```

## Megazord Maven details

The megazord Maven publication is a shell Android ARchive (AAR) that contains a native library and
depends on special `-withoutLibs` versions of the component modules.  For example, we have:

```
org.mozilla.appservices/logins.aar
- classes.jar
- libs/liblogins_ffi.so
```

and a `-withoutLibs` version, like:

```
org.mozilla.appservices/logins-withoutLibs.aar
- classes.jar
```

and then a megazord like:

```
org.mozilla.appservices/lockbox-megazord.aar
- libs/liblockbox.so
```

The `org.mozilla.appservices:lockbox-megazord` Maven publication then depends on
`org.mozilla.appservices:logins-withoutLibs` so that the JVM code (`classes.jar`) is used but
the component module native library (`libs/liblogins_ffi.so`) is not.

### Pseudo-code details

For each Android variant `variant`, the corresponding Gradle configurations
(`variant.{compileConfiguration,runtimeConfiguration}`) have module substitutions applied, like:

```groovy
// Pseudo-code!
configuration.resolutionStrategy.dependencySubstitution.all { dependency ->
    if (dependency.isComponentModule()) {
        dependency.useTarget('org.mozilla.appservices:example-megazord:...')
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
    configuration.name 'org.mozilla.appservices:places-forUnitTests:...'
}
```

When a megazord is used, the additional dependencies will be megazord-specific, like:


```groovy
// Pseudo-code!
dependencies {
    configuration.name 'org.mozilla.appservices:megazord-forUnitTests:...'
}
```
