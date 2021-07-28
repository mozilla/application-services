**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v81.0.1...main)

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

## General

### What's Changed

  - The bundled version of Glean has been updated to v39.0.4.

### What's New

  - Added content signature and chain of trust verification features in `rc_crypto` ([#4195](https://github.com/mozilla/application-services/pull/4195))
## Nimbus
### What's Changed
  - The Nimbus API now accepts application specific context as a part of its `appSettings`. The consumers get to define this context for targeting purposes. This allows different consumers to target on different fields without the SDK having to acknowledge all the fields. ([#4359](https://github.com/mozilla/application-services/pull/4359))
