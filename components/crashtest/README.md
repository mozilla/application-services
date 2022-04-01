# Crash Testing Helper APIs

This is a little helper component to make it easier to debug
issues with crash reporting, by letting you deliberately
crash a consuming app.

* [Features](#features)
* [Using the Crash Testing Helper APIs](#using-the-crash-testing-helper-apis)
* [Working on Crash Testing Helper APIs](#working-on-the-crash-testing-helper-apis)

## Features

The Crash Testing Helper APIs let you deliberately crash your application
in a variety of ways:

1. By triggering a hard abort inside the Rust code of the component, which
   should surface as a crash of the application.
1. By triggering a panic inside the Rust code of the component, which should
   surface as an "internal error" exception to the calling code.

The component does not offer any support for crash reporting, debugging etc
itself, it's just designed to let you more easily test those things in your
application.

## Using the Crash Testing Helper APIs

### Before using this component

This component does not currently integrate with the [Glean SDK](https://mozilla.github.io/glean/book/index.html)
and does not submit any telemetry, so you do not need to request a data-review before using this component.

### Prerequisites

To use this component, you should be familiar with how to integrate application-services components
into an application on your target platform:
* **Android**: Add the `mozilla.appservices.crashtest` package as a gradle dependency, but make sure
  you're using the same version of application-services as used by [Android Components](
  https://github.com/mozilla-mobile/android-components/tree/master/components/service/firefox-accounts/README.md).
* **iOS**: start with the [guide to consuming rust components on
  iOS](https://github.com/mozilla/application-services/blob/main/docs/howtos/consuming-rust-components-on-ios.md).
* **Other Platforms**: we don't know yet; please reach out on slack to discuss!

### Component API

For details on how to use this component, consult the [public interface definition](./src/crashtest.udl)
or view the generated Rust API documentation by running:

```
cargo doc --no-deps --open
```

## Working on the Crash Testing Helper APIs

### Prerequisites

To effectively work on the Crash Testing Helper APIs, you will need to be familiar with:

* Our general [guidelines for contributors](../../docs/contributing.md).
* The way we use [uniffi-rs](https://github.com/mozilla/uniffi-rs) to generate API wrappers for multiple languages, such as Kotlin and Swift.

### Overview

This component uses [uniffi-rs](https://github.com/mozilla/uniffi-rs) to create its
public API surface in Rust, and then generate bindings for Kotlin and Swift. The
code is organized as follows:

* The public API surface is implemented in [`./src/lib.rs`](./src/lib/rs), with matching
  declarations in [`./src/crashtest.udl`](./src/crashtest.udl) to define how it gets
  exposed to other languages.
* The [`./android/`](./android) directory contains android-specific build scripts that
  generate Kotlin wrappers and publish them as an AAR, and some Android tests.
* The [`./ios/`](./ios) directory is a placeholder for generated Swift code. There are
  a couple of Swift tests in `/megazords/ios-rust/MozillaTestServicesTests/CrashTestTests.swift`.

### Detailed Docs

We use rustdoc to document both the public API of this component and its
various internal implementation details. View the docs by running:

```
cargo doc --no-deps --document-private-items --open
```
