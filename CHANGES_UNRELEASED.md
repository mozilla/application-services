**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.3.2...main)

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
### What's fixed
- Fixed a bug released in 94.3.1. The bug broke firefox-ios builds due to a name conflict. ([#5181](https://github.com/mozilla/application-services/pull/5181))

### What's Changed
  - Updated UniFFI to 0.21.0.  This improves the string display of the fielded errors on Kotlin.  Currently only logins is using these errors, but we plan to start using them for all components.

## Autofill

### ⚠️ Breaking Changes ⚠️

   - The autofill API now uses `AutofillApiError` instead of `AutofillError`.   `AutofillApiError` exposes a smaller number of variants, which
     will hopefully make it easier to use for the consumer.

## Logins

### ⚠️ Breaking Changes ⚠️

   - Renamed `LoginsStorageError` to `LoginsApiError`, which better reflects how it's used and makes it consistent with
     the places error name.
   - Removed the `LoginsApiError::RequestFailed` variant.  This was only thrown when calling the sync-related methods
     manually, rather than going through the SyncManager which is the preferred way to sync. Those errors will now be
     grouped under `LoginsApiError::UnexpectedLoginsApiError`.

### What's Changed
  - Added fields to errors in `logins.udl`.  Most variants will now have a `message` field.

## Nimbus ⛅️🔬🔭

### What's Changed
  - Disabled Glean events recorded when the SDK is not ready for a feature ([#5185](https://github.com/mozilla/application-services/pull/5185))
  - Add struct for IntervalData (behavioral targeting) ([#5205](https://github.com/mozilla/application-services/pull/5205))
  - Calls to `log::error` have been replaced with `error_support::report_error` ([#5204](https://github.com/mozilla/application-services/pull/5204))

## Places

### ⚠️ Breaking Changes ⚠️

   - Renamed `PlacesError` to `PlacesApiError`, which better reflects that it's used in the public API rather than for
     internal errors.
   - Removed the `JsonError`, `InternalError`, and `BookmarksCorruption` variants from places error. Errors that
     resulted in `InternalError` will now result in `UnexpectedPlacesError`. `BookmarksCorruption` will also result in
     an `UnexpectedPlacesError` and an error report will be automatically generated. `JsonError` didn't seem to be
     actually used.

## Tabs

### ⚠️ Breaking Changes ⚠️

   - The tabs API now uses  `TabsError` with `TabsApiError`.  `TabsApiError` exposes a smaller number of variants, which
     will hopefully make it easier to use for the consumer.
