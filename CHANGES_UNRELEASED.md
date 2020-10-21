**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v66.0.0...main)

## iOS

## What's Changed

- The bundled version of Glean has been updated to v33.1.2.
  (as part of [#3701](https://github.com/mozilla/application-services/pull/3701))

## Android

## What's Changed

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
