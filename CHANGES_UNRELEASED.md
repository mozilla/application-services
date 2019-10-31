**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.2...master)

## Android

### What's new

- Exposed `stroage::bookmarks::erase_everything`, which deletes all bookmarks without affecting      history, through FFI.

## FxA Client

### What's new

Android: Add ability to get an OAuth code using a session token via the `authorizeOAuthCode` method.

## Push Client

### What's new

- `PushManager.dispatchInfoForChid(channelID)` now also returns the
  `endpoint` and `appServerKey` from the subscription.

### Breaking Changes

- the `appServerKey` VAPID public key has moved from `PushConfig` to
  `PushManager.subscription(channelID, scope, appServerKey)`.

- the unused `regenerate_endpoints()` function has been removed.

