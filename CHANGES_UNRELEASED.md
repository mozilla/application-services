**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v92.0.1...main)

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

## Nimbus ⛅️🔭🔬 + Nimbus FML ⛅️🔬🔭🔧

### What's New

- Add support for bundled resources in the FML in Swift. This corresponds to the `Image` and `Text` types. [#4892](https://github.com/mozilla/application-services/pull/4892)
  - This must include an update to the megazord, as well re-downloading the `nimbus-fml` binary.
  - Kotlin support for the same has also changed to match the Swift implementation, which has increased performance.
