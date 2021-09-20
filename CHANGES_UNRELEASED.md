**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.3.0...main)

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

## Sync

### What's Changed

- Clients engine now checks for tombstones and any deserialisation errors when receiving a client record, and ignores
  it if either are present ([#4504](https://github.com/mozilla/application-services/pull/4504))

## Nimbus
### What's changed
- The DTO changed to remove the `probeSets` and `enabled` fields that were previously unused. ([#4482](https://github.com/mozilla/application-services/pull/4482))
