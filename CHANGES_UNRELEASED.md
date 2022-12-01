**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.1.0...main)

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

## autofill

### What's Changed
  - Fixed a bug where `scrub_encrypted_data()` didn't update the last sync time, which prevented the scrubbed CC data
    from being fixed.
  - Don't report sentry errors when we try to decrypt the empty string.  This happens when the consumer tries to decript
    a CC number after `scrub_encrypted_data()` is called.

## places

### What's Changed
  - Switch to using incremental vacuums for maintenance, which should speed up the process.
