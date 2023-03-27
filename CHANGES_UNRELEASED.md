**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.2.0...main)

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

### ✨ What's New ✨

  - Added `recordPastEvent` for iOS and Android for testing of event store triggers. ([#5431](https://github.com/mozilla/application-services/pull/5431))
  - Added `recordMalformedConfiguration` method for `FeatureHolder` to record when some or all of a feature configuration is found to be invalid. ([#5440](https://github.com/mozilla/application-services/pull/5440))

### 🦊 What's Changed 🦊

  - Removed the check for major `schemaVersion` in Experiment recipes. ([#5433](https://github.com/mozilla/application-services/pull/5433))
