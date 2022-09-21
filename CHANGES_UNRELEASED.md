**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.2.0...main)

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
## Nimbus ⛅️🔬🔭

### What's Changed
 - Added `applyLocalExperiments()` method as short hand for `setLocalExperiments` and `applyPendingExperiments`. ([#5131](https://github.com/mozilla/application-services/pull/5131))
   - `applyLocalExperiments` and `applyPendingExperiments` now returns a cancellable job which can be used in a timeout.
   - `initialize` function takes a raw resource file id, and returns a cancellable `Job`.

### What's Fixed

   - A regression affecting Android in calculating `days_since_install` and `days_since_update` ([#5157](https://github.com/mozilla/application-services/pull/5157))
