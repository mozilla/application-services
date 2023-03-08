**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.1.0...main)

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

### ⚠️ Breaking Changes ⚠️

- Android: The JVM compatibility target is now version 11 ([#5401](https://github.com/mozilla/application-services/issues/5401))

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊
  - Fix Nimbus gradle plugin source file and task dependency issues ([#5421](https://github.com/mozilla/application-services/pull/5421))

### ✨ What's New ✨
  - Added new testing tooling `HardcodeNimbusFeatures` to aid UI and integration tests ([#5393](https://github.com/mozilla/application-services/pull/5393))
