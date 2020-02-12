**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.51.1...master)

## Sync

### What's changed

- Better caching of the tokenserver token and info/configuration response ([#2616](https://github.com/mozilla/application-services/issues/2616))

- Less network requests will be made in the case when nothing has changed on the server ([#2623](https://github.com/mozilla/application-services/issues/2623))

## Places

### What's changed

- Added a new field `reasons`, which is a `List` of `SearchResultReason`s, in `SearchResult`.

- Some places import related issues fixed ([#2536](https://github.com/mozilla/application-services/issues/2536),
  [#2607](https://github.com/mozilla/application-services/issues/2607))

### Breaking changes

- Android: The `PlacesWriterConnection.resetHistorySyncMetadata` and `PlacesWriterConnection.resetBookmarkSyncMetadata` methods have been moved to the `PlacesApi` class. ([#2668](https://github.com/mozilla/application-services/pull/2668))
- iOS: The `PlacesWriteConnection.resetHistorySyncMetadata` method has been moved to the `PlacesAPI` class. ([#2668](https://github.com/mozilla/application-services/pull/2668))

## FxA Client

### What's New

- Android: `FirefoxAccount.handlePushMessage` now handles all possible FxA push payloads and will return new `AccountEvent`s ([#2522](https://github.com/mozilla/application-services/pull/2522)):
  - `.ProfileUpdated` which should be handled by fetching the newest profile.
  - `.AccountAuthStateChanged` should be handled by checking if the authentication state is still valid.
  - `.AccountDestroyed` should be handled by removing the account information (no need to call `FirefoxAccount.disconnect`) from the device.
  - `.DeviceConnected` can be handled by showing a "<Device name> is connected to this account" notification.
  - `.DeviceDisconnected` should be handled by showing a "re-auth" state to the user if `isLocalDevice` is true. There is no need to call `FirefoxAccount.disconnect` as it will fail.

- iOS: Added `FxAccountManager.getSessionToken`. Note that you should request the `.session` scope in the constructor for this to work properly. ([#2638](https://github.com/mozilla/application-services/pull/2638))
- iOS: Added `FxAccountManager.getManageAccountURL`. ([#2658](https://github.com/mozilla/application-services/pull/2658))
- iOS: Added `FxAccountManager.getTokenServerEndpointURL`. ([#2658](https://github.com/mozilla/application-services/pull/2658))
- iOS: Added migration methods to `FxAccountManager` ([#2637](https://github.com/mozilla/application-services/pull/2637)):
  - `authenticateViaMigration` will try to authenticate an account without any user interaction using previously stored account information.
  - `accountMigrationInFlight` and `retryMigration` should be used in conjunction to handle cases where the migration could not be completed but is still recoverable.
- Added a `deviceId` property to the `AccountEvent.deviceDisconnected` enum case. ([#2645](https://github.com/mozilla/application-services/pull/2645))
- Added `context=oauth_webchannel_v1` in `getManageDevicesURL` methods for WebChannel redirect URLs. ([#2658](https://github.com/mozilla/application-services/pull/2658))

### Breaking changes

- Android: A few changes were made in order to decouple device commands from "account events" ([#2522](https://github.com/mozilla/application-services/pull/2522)):
  - `AccountEvent` enum has been refactored: `.TabReceived` has been replaced by `.IncomingDeviceCommand(IncomingDeviceCommand)`, `IncomingDeviceCommand` itself is another enum that contains `TabReceived`.
  - `FirefoxAccount.pollDeviceCommands` now returns an array of `IncomingDeviceCommand`.

- iOS: The `FxAccountManager` default applications scopes do not include `.oldSync` anymore. ([#2638](https://github.com/mozilla/application-services/pull/2638))

## Push

### What's New

- Android: Exposed `GeneralError` to the Kotlin layer.
