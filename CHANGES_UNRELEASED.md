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
## Places
### What's New
 - We Expose an API to set the Sync IDs for the history engine.
   This is to be used by iOS to supplement the migration and ensure that
   the history engine does not re-upload any already uploaded records.
