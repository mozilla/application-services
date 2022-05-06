**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.0.4...main)

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
## Nimbus ⛅️🔬🔭

### What's New
  - New API in the `FeatureHolder`, both iOS and Android to control the output of the `value()` call:
    - to cache the values given to callers; this can be cleared with `FxNimbus.invalidatedCachedValues()`
    - to add a custom initializer with `with(initializer:_)`/`withInitializer(_)`.
## Places
### What's Fixed:
- Fixed a bug in Android where non-fatal errors were crashing. ([#4941](https://github.com/mozilla/application-services/pull/4941))
- Fixed a bug where querying history metadata would return a sql error instead of the result ([4940](https://github.com/mozilla/application-services/pull/4940))
### What's new:
- Exposed the `deleteVisitsFor` function in iOS, the function can be used to delete history metadata. ([#4946](https://github.com/mozilla/application-services/pull/4946))
  - Note: The API is meant to delete all history, however, iOS does **not** use the `places` Rust component for regular history yet.
