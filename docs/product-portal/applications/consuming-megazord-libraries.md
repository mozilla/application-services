---
id: consuming-megazord-libraries
title: Consuming megazord libraries on Android
sidebar_label: Consuming megazord libraries
---

# Megazord libraries

The Rust component libraries that Application Services publishes stand alone: each published Android
ARchive (AAR) contains managed code (`classes.jar`) and multiple `.so` library files (one for each
supported architecture).  That means consuming multiple such libraries entails at least two `.so`
libraries, and each of those libraries includes the entire Rust standard library as well as
(potentially many) duplicated dependencies.  To save space and allow cross-component native-code
Link Time Optimization (LTO, i.e., inlining, dead code elimination, etc) Application Services also
publishes aggregate libraries -- so called *megazord libraries* -- that compose multiple Rust
components into a single optimized `.so` library file.  The managed code can be easily configured to
use such a megazord without significant changes.

There are two tasks we want to arrange.  First, we want to substitute component modules for the
single aggregate megazord module (a process that we call "megazording"); second, we want to arrange
for native Rust code to be available to JVM unit tests.  (They're related because the unit test
changes depend on the megazord used.)

Both tasks are handled by the
[org.mozilla.appservices](https://github.com/mozilla/application-services/gradle-plugin/README.md)
Gradle plugin.

# Consuming megazords

You'll need to:

1. Choose a megazord from the [list of megazords](#megazords) that Application Services produces in automation.
1. [Apply](#apply-the-gradle-plugin) the `org.mozilla.appservices` Gradle plugin.
1. [Configure](#configure-the-gradle-plugin) the Gradle plugin.
1. [Call `.init()`](#configuring-the-consuming-application) in your `Application.onCreate()`.
1. [Verify](#verify-that-your-apk-is-megazorded) that your APK is megazorded.

## Megazords

| Name | Components | Maven publication |
| --- | --- | --- |
| `lockbox` | `fxaclient`, `logins` | `org.mozilla.appservices:lockbox-megazord` |
| `reference-browser` | `fxaclient`, `logins`, `places` | `org.mozilla.appservices:reference-browser-megazord` |

If your project needs an additional megazord, talk to #rust-components on Slack.

## Apply the Gradle plugin

<a alt="Version badge" href="https://plugins.gradle.org/plugin/org.mozilla.appservices.gradle-plugin">
<img align="left" src="https://img.shields.io/maven-metadata/v/https/plugins.gradle.org/m2/org/mozilla/appservices/org.mozilla.appservices.gradle.plugin/maven-metadata.xml.svg?label=org.mozilla.appservices&colorB=brightgreen" />
</a>
<br/>

Build script snippet for plugins DSL for Gradle 2.1 and later:

```groovy
plugins {
  id 'org.mozilla.appservices' version '0.1.0'
}
```

Build script snippet for use in older Gradle versions or where dynamic configuration is required:

```groovy
buildscript {
  repositories {
    maven {
      url 'https://plugins.gradle.org/m2/'
    }
  }
  dependencies {
    classpath 'gradle.plugin.org.mozilla.appservices:gradle-plugin:0.1.0"
  }
}

apply plugin: 'org.mozilla.appservices'
```

## Configure the Gradle plugin

To consume a specific megazord module, use something like:

```groovy
appservices {
    defaultConfig {
        // Megazord in all Android variants.  The default is to not megazord.
        megazord = 'lockbox' // Or 'reference-browser', etc.
        enableUnitTests = false // Defaults to true.
    }
```

If you need, you can configure per Android variant: see the
[plugin docs](https://github.com/mozilla/application-services/gradle-plugin/README.md).

## Configuring the consuming application

The megazord modules expose a single additional JVM class, like
`org.mozilla.appservices.{Lockbox,ReferenceBrowser}Megazord`.  That class has a single static
`init()` method that consuming applications should invoke in their `Application.onCreate()` method,
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

        org.mozilla.appservices.LockboxMegazord.init();
    }

    ...
}
```

The `init()` method sets some Java system properties that tell the component modules which megazord
native code library contains the underlying component native code.

## Verify that your APK is megazorded

After `./gradlew app:assembleDebug`, list the contents of the APK produced.  For the Reference
Browser, this might be like:

```
./gradlew app:assembleGeckoNightlyArmDebug
unzip -l app/build/outputs/apk/geckoNightlyArm/debug/app-geckoNightly-arm-armeabi-v7a-debug.apk | grep lib/
```

You should see a single megazord `.so` library, like:

```
  5172812  00-00-1980 00:00   lib/armeabi-v7a/libreference_browser.so
```
and no additional _component_ `.so` libraries (like `libfxaclient_ffi.so`).  You will see additional
`.so` libraries -- just not component libraries, which are generally suffixed `_ffi.so`.

Then exercise your functionality on device and don't think about megazording again!
