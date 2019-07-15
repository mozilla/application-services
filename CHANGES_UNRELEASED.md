**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.36.0...master)

## FxA Client

### Breaking changes

- The `FirefoxAccount.beginOAuthFlow` method does not require the `wantsKeys` argument anymore
  as it will always do the right thing based on the requested scopes.
