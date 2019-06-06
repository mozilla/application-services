**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Cihangelog](https://github.com/mozilla/application-services/compare/v0.30.0...master)

## Sync

- Android: A new `sync15` package defines Kotlin data classes for the Sync
  telemetry ping.
- Android: `PlacesApi.syncHistory` and `PlacesApi.syncBookmarks` now return a
  `SyncTelemetryPing`.
- iOS: `PlacesAPI.syncBookmarks` now returns a JSON string with the contents of
  the Sync ping. This should be posted to the legacy telemetry submission
  endpoint.

## Logins

### Breaking Changes

- iOS: LoginsStoreError enum variants have their name `lowerCamelCased`
  instead of `UpperCamelCased`, to better fit with common Swift code style.
  ([#1042](https://github.com/mozilla/application-services/issues/1042))
