**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.53.1...master)

## FxA Client

### What's fixed

- iOS: `FxAccountManager.logout` will now properly clear the persisted account state. ([#2755](https://github.com/mozilla/application-services/issues/2755))
- iOS: `FxAccountManager.getAccessToken` now runs in a background thread. ([#2755](https://github.com/mozilla/application-services/issues/2755))
