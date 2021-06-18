**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v79.0.1...main)

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

## fxa-client
### ⚠️ Breaking Changes ⚠️
  - The old StateV1 persisted state schema is now removed. ([#4218](https://github.com/mozilla/application-services/pull/4218))
    Users on very old versions of this component will no longer be able to cleanly update to this version. Instead, the consumer code
    will recieve an error indicating that the schema was not correctly formated.

## Other
  - `./libs/build-all.sh` now displays a more helpful error message when a file fails checksum integrity test.
