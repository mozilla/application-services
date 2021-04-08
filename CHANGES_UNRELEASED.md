**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## History Metadata Storage

- Introduced a new metadata storage API, part of libplaces. Currently only has Android bindings.

## Sync Manager

- Removed support for the wipeAll command (#4006)

## Autofill

### What's Changed

- Added support to scrub encrypted data to handle lost/corrupted client keys.
  Scrubbed data will be replaced with remote data on the next sync.

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.2.0...main)
