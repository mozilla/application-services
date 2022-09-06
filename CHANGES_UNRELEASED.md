**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.1.0...main)

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
# General
  - `error-support` is now exposed to both Firefox iOS and Focus iOS. `error-support` supports better error reporting and logging for errors. ([#5094](https://github.com/mozilla/application-services/pull/5094))

## Nimbus FML ⛅️🔬🔭🔧
### What's Changed
  - Add `channels` value for defaults and add support for multiple channels in `channel` via comma separation. ([#5101](https://github.com/mozilla/application-services/pull/5101))

### ✨ What's New ✨
  - JEXL targeting allows for using the `in` keyword with objects and a map of active experiments has been added to TargetingAttributes. The map will always be empty at this time. ([#5104](https://github.com/mozilla/application-services/pull/5104))

## Nimbus ⛅️🔬🔭

### What's Changed
 - Added metrics for SDK unavailability and disk cache unreadiness to tease apart the difference in startup slowness. ([#5118](https://github.com/mozilla/application-services/pull/5118))
