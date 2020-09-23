**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v63.0.0...main)

## iOS

### ⚠️ Breaking changes ⚠️

- The `MozillaAppServices.framework` is now built using Xcode 12, so consumers will need
  update their own build accordingly.
  ([#3586](https://github.com/mozilla/application-services/pull/3586))

## Autofill

### What's changed ###
- Added a basic API and database layer for the autofill component. ([#3582](https://github.com/mozilla/application-services/pull/3582))

## Places

### What's changed
- Removed the duplicate Timestamp logic from Places, which now exists in Support, and updated the references. ([#3593](https://github.com/mozilla/application-services/pull/3593))
