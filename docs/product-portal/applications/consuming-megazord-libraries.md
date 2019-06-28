---
id: consuming-megazord-libraries
title: Consuming megazord libraries on Android
sidebar_label: Consuming megazord libraries
---

# Megazord libraries

Each Rust component published by Application Services is conceptually a stand-alone library, but they
all depend on a shared core of functionality for exposing Rust to Kotlin.  In order to allow easy interop
between components, enable cross-component native-code Link Time Optimization, and reduce final application
size, the rust code for all components is compiled and distributed together as a single aggregate library
which we have dubbed a ***megazord library***.

Each Application Services component is published as an Android ARchive (AAR) that contains the managed code
for that component (`classes.jar`) and which depends on a separate "megazord" AAR that contains all of the
compiled rust code (`libmegazord.so`). For an application that consumes multiple Application Services components,
the dependency graph thus looks like this:

[![megazord dependency diagram](https://docs.google.com/drawings/d/e/2PACX-1vTA6wL3ibJRNjKXsmescTfKTx0w_fpr5NcDIF_4T5AsnZfCi8UEEcav8vibocSyKpHOQOk5ysiDBm-D/pub?w=727&h=546)](https://docs.google.com/drawings/d/1owo4wo2F1ePlCq2NS0LmAOG4jRoT_eVBahGNeWHuhJY/)

While this setup is *mostly* transparent to the consuming application, there are a few points to be aware of
which are outlined below.

## Initializing Shared Infrastructure

The megazord AAR exposes a single additional JVM class, `mozilla.appservices.Megazord`, which the application
should initialize explicitly. This would typically be done in the `Application.onCreate()` method, like so:

```kotlin
import mozilla.appservices.Megazord

open class Application extends android.app.Application {
    override fun onCreate() {
        super.onCreate();
        Megazord.init();
    }
    ...
}
```

The `init()` method sets some Java system properties that help the component modules locate the compiled
rust code.

After initializing the Megazord, the application can configure shared infrastructure such as logging:

```kotlin
import mozilla.components.support.rustlog.RustLog

open class Application extends android.app.Application {
    override fun onCreate() {
      ...
      Megazord.init();
      ...
      RustLog.enable()
      ...
    }
}
```

Or networking:

```kotlin
import mozilla.components.lib.fetch.httpurlconnection.HttpURLConnectionClient
import mozilla.appservices.httpconfig.RustHttpConfig

open class Application extends android.app.Application {
    override fun onCreate() {
      ...
      Megazord.init();
      ...
      RustHttpConfig.setClient(lazy { HttpURLConnectionClient() })
      ...
    }
}
```

The configured settings will then be used by all rust components provided by the megazord.

## Using a custom Megazord

The default megazord library contains compiled rust code for *all* components published by Application Services.
If the consuming application only uses a subset of those components, it's possible to its package size and load
time by using a custom-built megazord library containing only the required components.

First, you will need to select an appropriate custom megazord. Application Services publishes several custom megazords
to fit the needs of existing Firefox applications:

| Name | Components | Maven publication |
| --- | --- | --- |
| `lockbox` | `fxaclient`, `logins` | `org.mozilla.appservices:lockbox-megazord` |
| `fenix` | `fxaclient`, `logins`, `places` | `org.mozilla.appservices:fenix-megazord` |

Then, simply use gradle's builtin support for [dependency
substitution](https://docs.gradle.org/current/dsl/org.gradle.api.artifacts.DependencySubstitutions.html)
to replace the default "full megazord" with your selected custom build:

```groovy
configurations.all {
  resolutionStrategy.dependencySubstitution {
    substitute module("org.mozilla.appservices:full-megazord:X.Y.Z") with module("org.mozilla.appservices:fenix-megazord:X.Y.Z")
  }
}
```

If you would like a new custom megazord for your project, please reach out via #rust-components in slack.

## Running unit tests

Since the megazord library contains compiled native code, it cannot be used directly for running local unittests
(it's compiled for the android target device, not for your development host machine). To support running unittests
via the JVM on the host machine, we publish a special `forUnitTests` variant of the megazord library in which the
native code is compiled into a JAR for common desktop architectures.

Use dependency substitution to include it in your test configuration as follows:

```groovy
configurations.testImplementation.resolutionStrategy.dependencySubstitution {
  substitute module("org.mozilla.appservices:full-megazord:X.Y.Z") with module("org.mozilla.appservices:full-megazord-forUnitTests:X.Y.Z")
}
```

If you are using a custom megazord library, substitute both the default and custom module with the `forUnitTests`
variant of your custom megazord:


```groovy
configurations.testImplementation.resolutionStrategy.dependencySubstitution {
  substitute module("org.mozilla.appservices:full-megazord:X.Y.Z") with module("org.mozilla.appservices:fenix-megazord-forUnitTests:X.Y.Z")
  substitute module("org.mozilla.appservices:fenix-megazord:X.Y.Z") with module("org.mozilla.appservices:fenix-megazord-forUnitTests:X.Y.Z")
}
```