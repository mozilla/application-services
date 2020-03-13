**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.54.1...master)

## Places

### ⚠️ Breaking changes ⚠️

- Android: `PlacesConnection.deletePlace` has been renamed to
  `deleteVisitsFor`, to clarify that it might not actually delete the
  page if it's bookmarked, or has a keyword or tags
  ([#2695](https://github.com/mozilla/application-services/pull/2695)).

### What's fixed

- `history::delete_visits_for` (formerly `delete_place_by_guid`) now correctly
  deletes all visits from a page if it has foreign key references, like
  bookmarks, keywords, or tags. Previously, this would cause a constraint
  violation ([#2695](https://github.com/mozilla/application-services/pull/2695)).

## FxA Client

### What's new

- Added `getPairingAuthorityURL` method returning the URL the user should navigate to on their Desktop computer to perform a pairing flow. ([#2815](https://github.com/mozilla/application-services/pull/2815))

### Breaking changes

- In order to account better for self-hosted FxA/Sync backends, the FxAConfig objects have been reworked. ([#2801](https://github.com/mozilla/application-services/pull/2801))
  - iOS: `FxAConfig.release(contentURL, clientID)` is now `FxAConfig(server: .release, contentURL, clientID)`.
  - Android: `Config.release(contentURL, clientID)` is now `Config(Server.RELEASE, contentURL, clientID)`.
  - These constructors also take a new `tokenServerUrlOverride` optional 4th parameter that overrides the token server URL.

- iOS: `FxAccountManager`'s `getManageAccountURL` and `getTokenServerEndpointURL` methods now run on background thread and return their results in a callback function. ([#2813](https://github.com/mozilla/application-services/pull/2813))
