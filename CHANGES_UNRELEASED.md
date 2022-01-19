**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v88.0.0...main)

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
## Supported Xcode Versions
- Reverting the supported Xcode version from 13.2.1 to 13.1.0 to circumvent the issues with Swift Package Manager in Xcode 13.2.1. ([#4787](https://github.com/mozilla/application-services/pull/4787))
## Nimbus☁️🔬🔭

### What's New
   - Add `Text` and `Image` support for the FML to access bundled resources ([#4784](https://github.com/mozilla/application-services/pull/4784)).

### Breaking Change
  - The `NimbusInterface` now exposes a `context: Context` property.