**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.54.0...master)

## Sync

### What's fixed

- Engine disabled/enabled state changes now work again after a regression in
  0.53.0.

## Android

### What's changed

- There is now preliminary support for an "autoPublish" local-development workflow similar
  to the one used when working with Fenix and android-components; see
  [this howto guide](./docs/howtos/locally-published-components-in-fenix.md) for details.

## Places

### What's fixed

- Improve handling of bookmark search keywords. Keywords are now imported
  correctly from Fennec, and signing out of Sync in Firefox for iOS no longer
  loses keywords ([#2501](https://github.com/mozilla/application-services/pull/2501)).
