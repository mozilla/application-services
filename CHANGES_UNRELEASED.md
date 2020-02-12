**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.51.1...master)

## Places

### What's changed

- Added a new field `reasons`, which is a `List` of `SearchResultReason`s, in `SearchResult`.

## FxA Client

### What's New

- Android: `FirefoxAccount.handlePushMessage` now handles all possible FxA push payloads and will return new `AccountEvent`s ([#2522](https://github.com/mozilla/application-services/pull/2522)):
  - `.ProfileUpdated` which should be handled by fetching the newest profile.
  - `.AccountAuthStateChanged` should be handled by checking if the authentication state is still valid.
  - `.AccountDestroyed` should be handled by removing the account information (no need to call `FirefoxAccount.disconnect`) from the device.
  - `.DeviceConnected` can be handled by showing a "<Device name> is connected to this account" notification.
  - `.DeviceDisconnected` should be handled by showing a "re-auth" state to the user if `isLocalDevice` is true. There is no need to call `FirefoxAccount.disconnect` as it will fail.

- iOS: Added `FxAccountManager.getSessionToken`. Note that you should request the `.session` scope in the constructor for this to work properly ([#2638](https://github.com/mozilla/application-services/pull/2638))

### Breaking changes

- Android: A few changes were made in order to decouple device commands from "account events" ([#2522](https://github.com/mozilla/application-services/pull/2522)):
  - `AccountEvent` enum has been refactored: `.TabReceived` has been replaced by `.IncomingDeviceCommand(IncomingDeviceCommand)`, `IncomingDeviceCommand` itself is another enum that contains `TabReceived`.
  - `FirefoxAccount.pollDeviceCommands` now returns an array of `IncomingDeviceCommand`.

- iOS: The `FxAccountManager` default applications scopes do not include `.oldSync` anymore. ([#2638](https://github.com/mozilla/application-services/pull/2638))
