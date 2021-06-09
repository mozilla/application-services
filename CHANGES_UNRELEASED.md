**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v78.0.0...main)

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

## Logins

### ⚠️ Breaking changes ⚠️

Logins now Uniffi-ed! While this is a very large change internally, the externally visible changes are:

- The name and types of exceptions have changed - the base class for errors is LoginsStorageErrorException.
- The struct `ServerPassword` (Android) and `LoginRecord` (iOS) is now named `Login` with the formSubmitURL field now formSubmitUrl

## [viaduct-reqwest]

### What's Changed

- Update viaduct-reqwest to use reqwest 0.11. ([#4146](https://github.com/mozilla/application-services/pull/4146))
