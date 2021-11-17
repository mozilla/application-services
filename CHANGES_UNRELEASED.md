**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v86.2.0...main)

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
### What's changed
  - Push internally no longer uses the `error_support` dependency to simplify the code. It now directly defines exactly one error enum and exposes that to `uniffi`. This should have no implication to the consumer code ([#4650](https://github.com/mozilla/application-services/pull/4650))

## Places
### ⚠️ Breaking Changes ⚠️
  - Switched sync manager integration to use `registerWithSyncManager()` like the other components ([#4627](https://github.com/mozilla/application-services/pull/4627))

## SyncManager

### ⚠️ Breaking Changes ⚠️
  - Updated SyncManager to use UniFFI:
    - SyncManager is now a class that gets instatiated rather than a singleton
    - Added more SyncManagerException subclasses
    - SyncParams.engines is now a SyncEngineSelection enum.
      SyncEngineSelection's variants are All, or Some(engine_list).  This
      replaces the old code which used null to signify all engines.
    - SyncResult.telemetry was replaced with SyncResult.telemetryJson.
    - There were a handful of naming changes:
      - SyncAuthInfo.tokenserverURL -> SyncAuthInfo.tokenserverUrl
      - DeviceSettings.type -> DeviceSettings.kind
