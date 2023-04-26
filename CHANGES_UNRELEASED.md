**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.5.1...main)

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

- Added processing of command line arguments (or intent extras) to be driven by a command line tool. ([#5482](https://github.com/mozilla/application-services/pull/5482), [#5497](https://github.com/mozilla/application-services/pull/5497))
  - Requires passing `CommandLine.arguments` to `NimbusBuilder` in iOS.
  - Requires passing `intent` to `NimbusInterface` in Android.
- Added Cirrus client object for working with Nimbus in a static, stateless manner ([#5471](https://github.com/mozilla/application-services/pull/5471)).

## Places ⛅️🔬🔭

### 🦊 What's Changed 🦊

  - Added support for sync payload evolution in history.  If other clients sync history records / visits with fields that we don't know about, we store that data as JSON and send it back when it's synced next.
