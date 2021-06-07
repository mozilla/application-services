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

## [viaduct-reqwest]

### What's Changed

- Update viaduct-reqwest to use reqwest 0.11. ([#4146](https://github.com/mozilla/application-services/pull/4146))

## Logins

### ⚠️ Breaking changes ⚠️

Logins now Uniffi-ed!

API Changes for Logins components:

- Login is the main struct moving forward
  - Previously Android had `ServerPassword` and iOS had `LoginRecord`
  - `id` is now a String for consumers but internall we call `guid()` to generate/fetch the value
- `PasswordStore` is renamed to `LoginStore` and is the consumer facing store
  - The previous `LoginStore` in db.rs is more aptly named `LoginsSyncEngine`
- Throwing exceptions is now done via [likely name change] LoginsStorageErrorException
