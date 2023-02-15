**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.4.0...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### 🦊 What's Changed 🦊
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### ✨ What's New ✨
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## Nimbus ⛅️🔬🔭
### 🦊 What's Changed 🦊
- Updated the Nimbus Gradle Plugin to fix a number of issues after migrating it to this repository ([#5348](https://github.com/mozilla/application-services/pull/5348))
- Good fences: protected calls out to the error reporter with a `try`/`catch` ([#5366](https://github.com/mozilla/application-services/pull/5366))
- Updated the Nimbus FML CLI to only import the R class if it will be used by a feature property ([#5361](https://github.com/mozilla/application-services/pull/5361))
### ⚠️ Breaking Changes ⚠️
  - Android and iOS: Several errors have been moved to an internal support library and will no longer be reported as top-level Nimbus errors. They should still be accessible through `NimbusError.ClientError`. They are: `RequestError`, `ResponseError`, and `BackoffError`. ([#5369](https://github.com/mozilla/application-services/pull/5369))
