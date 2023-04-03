**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.3.0...main)

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

## General

### What's Changed

- Android: Upgraded NDK from r21d to r25c.

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊
- Added a `nimbus` cargo feature and added appropriate feature flag attributes ([#5448](https://github.com/mozilla/application-services/pull/5448)).
  - This does not functionally change build processes, as the `nimbus` feature is now the default feature for the `nimbus-sdk` library.
- Changed the ordering around for optional arguments for Python compatibility ([#5460](https://github.com/mozilla/application-services/pull/5460)).
  - This does not change Kotlin or Swift APIs, but affects code that uses the uniffi generated FFI for `record_event` and `record_past_event` directly.
