**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## Autofill

### What's Changed

- Added support to scrub encrypted data to handle lost/corrupted client keys.
  Scrubbed data will be replaced with remote data on the next sync.

## Nimbus

### ⚠️ Breaking changes ⚠️

- Moved the `Nimbus` class and its test class from Android Components into this repository. For
  compatibility with existing integrations, this remains in the same package. These changes are
  fixed in the [android-components#10144](https://github.com/mozilla-mobile/android-components/pull/10144)

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.2.0...main)
