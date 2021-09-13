**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v83.0.0...main)

## Places

### ⚠️ Breaking Changes ⚠️
  - `previewImageUrl` property was added to `HistoryMetadata` ([#4448](https://github.com/mozilla/application-services/pull/4448))
### What's changed
  - `previewImageUrl` was added to `VisitObservation`, allowing clients to make observations about the 'hero' image of the webpage ([#4448](https://github.com/mozilla/application-services/pull/4448))

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
## Push
### ⚠️ Breaking Changes ⚠️
  - The push component now uses `uniffi`! Here are the Kotlin breaking changes related to that:
     - `PushAPI` no longer exists, consumers should consumer `PushManager` directly
     - `PushError` becomes `PushException`, and all specific errors are now `PushException` children, and can be retrieved using `PushException.{ExceptionName}`, for example `StorageError` becomes `PushException.StorageException`
     - The `PushManager.decrypt` function now returns a `List<Byte>`, where it used to return `ByteArray`, the consumer can do the conversion using `.toByteArray()`
     - All references to `channelID` become `channelId` (with a lowercase `d`)