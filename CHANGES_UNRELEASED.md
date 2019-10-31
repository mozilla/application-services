**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.2...master)

## Push Client

### What's new

- `PushManager.dispatchInfoForChid(channelID)` now also returns the
  `endpoint` and `appServerKey` from the subscription.

### Breaking Changes

- the `appServerKey` VAPID public key has moved from `PushConfig` to
  `PushManager.subscription(channelID, scope, appServerKey)`.

- the unused `regenerate_endpoints()` function has been removed.

