**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.34.0...master)

## FxA Client

### Breaking changes

- The `FirefoxAccount.destroyDevice` method has been removed in favor of the
  more general `FirefoxAccount.disconnect` method which will ensure a full
  disconnection by invalidating OAuth tokens and destroying the device record
  if it exists. ([#1397](https://github.com/mozilla/application-services/issues/1397))
- The `FirefoxAccount.disconnect` method has been added to the Swift bindings as well.
