**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.53.2...master)
## Push

### Breaking changes

- Android: The `PushManager.verifyConnection` now returns a `List<SubscriptionChanged>` that contain the channel ID and scope of the subscriptions that have expired.
  See [`onpushsubscriptionchange`][0] events on how this change can be propagated to notify web content.

[0]: https://developer.mozilla.org/en-US/docs/Web/API/ServiceWorkerGlobalScope/onpushsubscriptionchange
