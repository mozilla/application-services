**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.2.0...main)

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

## General

### What's Changed

- Rust toolchain has been bumped to 1.63 and minimum version bumped to 1.61 to comply with our [Rust Policy](https://github.com/mozilla/application-services/blob/main/docs/rust-versions.md#application-services-rust-version-policy)
- Android: Upgraded NDK from r21d to r25b. ([#5142](https://github.com/mozilla/application-services/pull/5142))

## Places

### What's Changed
 - Added metrics for the `run_maintenance()` method.  This can be used by consumers to decide when to schedule the next `run_maintenance()` call and to check if calls are taking too much time.

### What's new
  - Exposed a function in Swift `migrateHistoryFromBrowserDb` to migrate history from `browser.db` to `places.db`, the function will migrate all the local visits in one go. ([#5077](https://github.com/mozilla/application-services/pull/5077)).
    - The migration might take some time if a user had a lot of history, so make sure it is **not** run on a thread that shouldn't wait.
    - The migration runs on a writer connection. This means that other writes to the `places.db` will be delayed until the migration is done.

## Nimbus

### What's Changed
 - Added `applyLocalExperiments()` method as short hand for `setLocalExperiments` and `applyPendingExperiments`. ([#5131](https://github.com/mozilla/application-services/pull/5131))
   - `applyLocalExperiments` and `applyPendingExperiments` now returns a cancellable job which can be used in a timeout.
   - `initialize` function takes a raw resource file id, and returns a cancellable `Job`.
