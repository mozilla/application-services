**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v80.0.1...main)

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
## Nimbus
### What's changed
  - Fixed a bug where opt-in enrollments in experiments were not preserved when the application is restarted ([#4324](https://github.com/mozilla/application-services/pull/4324))
  - The nimbus component now specifies the version of the server's api - currently V1. That was done to avoid redirects. ([#4319](https://github.com/mozilla/application-services/pull/4319))

## Push
### What's changed
  - Fixed a bug where we don't delete a client's UAID locally when it's deleted on the server ([#4325](https://github.com/mozilla/application-services/pull/4325))

## Tabs
### ⚠️ Breaking Changes ⚠️
 - Tabs has been Uniffi-ed! ([#4192](https://github.com/mozilla/application-services/pull/4192))
