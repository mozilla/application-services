**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v66.0.0...main)

## General

### ⚠️ Breaking changes ⚠️

- The custom "Lockbox Megazord" package (`org.mozilla.appservices:lockbox-megazord`) has been removed.
  Existing consumers of this package who wish to update to the latest release of application-services
  should migrate to using the default `appservices:full-megazord` package, or contact the development
  team to discuss an alternate approach.
  ([#3700](https://github.com/mozilla/application-services/pull/3700))

### What's Changed

- The version of Rust used to compile our components has been pinned to v1.43.0 in order to match
  the version of Rust used in mozilla-central. Changes that do not compile under this version of
  Rust will not be accepted.
  ([#3702](https://github.com/mozilla/application-services/pull/3702))

## iOS

### What's Changed

- The bundled version of Glean has been updated to v33.1.2.
  (as part of [#3701](https://github.com/mozilla/application-services/pull/3701))

## Android

### What's Changed

- This release comes with a nontrivial increase in the compiled code size of the
  `org.mozilla.appservices:full-megazord` package, adding approximately 1M per platform
  thanks to the addition of the Nimbus SDK component.
  ([#3701](https://github.com/mozilla/application-services/pull/3701))
- Several core gradle dependencies have been updated, including gradle itself (now v6.5)
  and the android gradle plugin (now v4.0.1).
  ([#3701](https://github.com/mozilla/application-services/pull/3701))

## Nimbus SDK

### What's New

- The first version of the Nimbus Experimentation SDK is now available, via the
  `org.mozilla.appservices:nimbus` package. More details can be found in the
  [nimbus-sdk repo](https://github.com/mozilla/nimbus-sdk).
  ([#3701](https://github.com/mozilla/application-services/pull/3701))

## FxA Client

### What's Fixed

- We no longer discard the final path component from self-hosted sync tokenserver URLs.
  ([#3694](https://github.com/mozilla/application-services/pull/3694))

## Autofill

### What's Changed

- We added the `touch_address` and `touch_credit_card` store functions and refactored the component.
  ([#3691](https://github.com/mozilla/application-services/pull/3691))

## Push

### What's Changed

- Attempts to update the device push token are now rate-limited.
  ([#3673](https://github.com/mozilla/application-services/pull/3673))

## WebExtension Storage

### What's Fixed

- Syncing of incoming tombstone records has been fixed; previously the presence
  of an incoming tombstone could cause the sync to fail.
  ([#3668](https://github.com/mozilla/application-services/pull/3668))

