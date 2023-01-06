**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.2.1...main)

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

## Places
### What's changed
 - Removes old iOS bookmarks migration code. The function `migrateBookmarksFromBrowserDb` no longer exists. ([#5276](https://github.com/mozilla/application-services/pull/5276))

## Nimbus ⛅️🔬🔭
### What's New
  - iOS: added a `Bundle.fallbackTranslationBundle()` method. ([#5314](https://github.com/mozilla/application-services/pull/5314))
