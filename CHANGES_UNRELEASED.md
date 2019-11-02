**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.2...master)

## General

### What's New

- On Android, our Megazord libraries now include license information for dependencies
  as part of their `.pom` file, making it easily available to tools such as the
   [oss-licenses-plugin](https://github.com/google/play-services-plugins/tree/master/oss-licenses-plugin)

## Push Client

### What's new

- `PushManager.dispatchInfoForChid(channelID)` now also returns the
  `endpoint` and `appServerKey` from the subscription.

### Breaking Changes

- the `appServerKey` VAPID public key has moved from `PushConfig` to
  `PushManager.subscription(channelID, scope, appServerKey)`.

- the unused `regenerate_endpoints()` function has been removed.

