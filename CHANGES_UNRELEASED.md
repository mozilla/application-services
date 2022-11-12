**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v95.0.1...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's Changed
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### ✨ What's New ✨
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## ⛅️🔬🔭 Nimbus

### ✨ What's New ✨
  - `active_experiments` is available to JEXL as a set containing slugs of all enrolled experiments ([#5227](https://github.com/mozilla/application-services/pull/5227))
  - Added query method for behavioral targeting event store ([#5226](https://github.com/mozilla/application-services/pull/5226))

### ⚠️ Breaking Changes ⚠️
  - Changed the type of `customTargetingAttributes` in `NimbusAppSettings` to a `JSONObject`. The change will be breaking only for Android. ([#5229](https://github.com/mozilla/application-services/pull/5229))
