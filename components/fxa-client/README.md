# Firefox Accounts Client

The fxa-client component lets applications integrate with the
[Firefox Accounts](https://mozilla.github.io/ecosystem-platform/docs/features/firefox-accounts/fxa-overview)
identity service.

* [Features](#features)
* [Using the Firefox Accounts client](#using-the-firefox-accounts-client)
* [Working on the Firefox Accounts client](#working-on-the-firefox-accounts-client)

## Features

The fxa-client component offers support for:

1. Letting users sign in to your app,
   using either an email-and-password-based OAuth flow
   or a QR-code-based pairing flow.
1. Accessing basic profile information about the signed-in user,
   such as email address and display name.
1. Obtaining OAuth access tokens and client-side encryption keys,
   in order to access account-enabled services such as Firefox Sync.
1. Listing and managing other applications connected to the
   user's account.
1. Sending tabs to other applications that are capable
   of receiving them.
1. Managing a device record for your signed-in application,
   making it visible to other applications connected to the user's
   account.

The component ***does not*** offer, and we have no concrete plans to offer:

* The ability to store data "in the cloud" associated with the user's account.

## Using the Firefox Accounts client

### Before using this component

This component does not currently integrate with the [Glean SDK](https://mozilla.github.io/glean/book/index.html)
and does not submit any telemetry, so you do not need to request a data-review before using this component.

### Prerequisites

To use this component, your application must be registered to [integrate with Firefox Accounts
as an OAuth client](https://mozilla.github.io/ecosystem-platform/relying-parties/tutorials/integration-with-fxa)
and have a unique OAuth `client_id`.

You should also be familiar with how to integrate application-services components
into an application on your target platform:
* **Android**: integrate via the
  [service-firefox-accounts](https://github.com/mozilla-mobile/android-components/tree/master/components/service/firefox-accounts/README.md)
  component from android-components, which provides higher-level conveniences for state management, persistence,
  and integration with other Android components.
* **iOS**: start with the [guide to consuming rust components on
  iOS](https://github.com/mozilla/application-services/blob/main/docs/howtos/consuming-rust-components-on-ios.md)
  and take a look at the [higher-level Swift wrapper classes](./ios/FxAClient/).
* **Other Platforms**: we don't know yet; please reach out on slack to discuss!

### Core Concepts

You should understand the core concepts of OAuth and the Firefox Accounts system
before attempting to use this component. Please review the
[Firefox Accounts Documentation](https://mozilla.github.io/ecosystem-platform/docs/features/firefox-accounts/fxa-overview)
for more details.

In particular, you should understand [the different types of auth token](
https://github.com/mozilla/ecosystem-platform/pull/39)
in the FxA ecosystem and how each is used, as well as how [OAuth scopes](
https://github.com/mozilla/fxa/blob/main/packages/fxa-auth-server/docs/oauth/scopes.md)
work for accessing related services.

### Component API

For details on how to use this component, consult the [public interface definition](./src/fxa_client.udl) or view the generated Rust API documentation by running:

```
cargo doc --no-deps --open
```

## Working on the Firefox Accounts client

### Prerequisites

To effectively work on the FxA Client component, you will need to be familiar with:

* Our general [guidelines for contributors](../../docs/contributing.md).
* The [core concepts](#core-concepts) for users of the component, outlined above.
* The way we use [uniffi-rs](https://github.com/mozilla/uniffi-rs) to generate API wrappers for multiple languages, such as Kotlin and Swift.

### Overview

This component uses [uniffi-rs](https://github.com/mozilla/uniffi-rs) to create its
public API surface in Rust, and then generate bindings for Kotlin and Swift. The
code is organized as follows:

* The public API surface is implemented in [`./src/lib.rs`](./src/lib/rs), with matching
  declarations in [`./src/fxa_client.udl`](./src/fxa_client.udl) to define how it gets
  exposed to other languages.
    * All the implementation details are written in Rust and can be found under
      [`./src/internal/`](./src/internal).
* The [`./android/`](./android) directory contains android-specific build scripts that
  generate Kotlin wrappers and publish them as an AAR. It also contains a small amount
  of hand-written Kotlin code for things that are not yet supposed by UniFFI.
* The [`./ios/`](./ios) directory contains ios-specific build scripts that generate
  Swift wrappers for consumption via an Xcode build. It also contains some hand-written
  Swift code to expose a higher-level convenience API.

### Detailed Docs

We use rustdoc to document both the public API of this component and its
various internal implementation details. View the docs by running:

```
cargo doc --no-deps --document-private-items --open
```

In particular, the `internal` sub-module contains most of the business
logic, and is delegated to from the public API.
