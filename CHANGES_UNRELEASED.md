**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.6.0...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's Changed
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### ✨ What's New ✨
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## Nimbus FML ⛅️🔬🔭🔧
### What's Changed
  - Added `MOZ_APPSERVICES_MODULE` environment variable to specify the megazord module for iOS ([#5042](https://github.com/mozilla/application-services/pull/5042)). If it is missing, no module is imported.
### ✨ What's New ✨
  - Enabled remote loading and using configuring of branches. ([#5041](https://github.com/mozilla/application-services/pull/5041))
  - Add a `fetch` command to `nimbus-fml` to demo and test remote loading and paths. ([#5047](https://github.com/mozilla/application-services/pull/5047))

## Logins
### What's Changed
  - Updated the `LoginsStorageError` implementation and introduce error reporting for unexpected errors.
    Note that some errors were removed, which is technically a breaking change, but none of our
    consumers use those errors so it's not a breaking change in practice.
