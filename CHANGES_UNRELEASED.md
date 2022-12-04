**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.1.1...main)

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

## Logins
### What's changed
 - Removes Fennec migration code. The function `importMultiple` no longer exists. ([#5268](https://github.com/mozilla/application-services/pull/5268))

## Places
### What's changed
 - Removes Fennec migration code. ([#5268](https://github.com/mozilla/application-services/pull/5268))
  The following functions no longer exist: 
   - `importBookmarksFromFennec`
   - `importPinnedSitesFromFennec`
   - `importVisitsFromFennec`

## Viaduct
### What's New
  - Allow viaduct to make requests to the android emulator's host address via
    a new viaduct_allow_android_emulator_loopback() (in Rust)/allowAndroidEmulatorLoopback() (in Kotlin)
    ([#5270](https://github.com/mozilla/application-services/pull/5270))

## Tabs
### What's changes
  - The ClientRemoteTabs struct/interface now has a last_modified field which is the time
    when the device last uploaded the tabs.
