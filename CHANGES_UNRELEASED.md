**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v92.0.0...main)

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

## Nimbus FML ⛅️🔬🔭🔧

### What's Fixed

- Swift: a bug in our understanding of Swift optional chaining rules meant that maps with a mapping and merging produced invalid code. ([#4885](https://github.com/mozilla/application-services/pull/4885))

## General

### What's Changed

- Added documentation of our sqlite pragma usage. ([#4876](https://github.com/mozilla/application-services/pull/4876))
