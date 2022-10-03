**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.3.2...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's Changed
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's New
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## General
### What's fixed
- Fixed a bug released in 94.3.1. The bug broke firefox-ios builds due to a name conflict. ([#5181](https://github.com/mozilla/application-services/pull/5181))

### What's Changed
  - Updated UniFFI to 0.21.0.  This improves the string display of the fielded errors on Kotlin.  Currently only logins is using these errors, but we plan to start using them for all components.

## Nimbus ⛅️🔬🔭

### What's Changed
  - Disabled Glean events recorded when the SDK is not ready for a feature ([#5185](https://github.com/mozilla/application-services/pull/5185))

## Places

### ⚠️ Breaking Changes ⚠️

   - Renamed `PlacesError` to `PlacesApiError`, which better reflects that it's used in the public API rather than for
     internal errors.
   - Removed the `JsonError`, `InternalError`, and `BookmarksCorruption` variants from places error. Errors that
     resulted in `InternalError` will now result in `UnexpectedPlacesError`. `BookmarksCorruption` will also result in
     an `UnexpectedPlacesError` and an error report will be automatically generated. `JsonError` didn't seem to be
     actually used.
