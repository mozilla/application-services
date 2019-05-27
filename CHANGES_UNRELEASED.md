**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## FxA Client

### What's New

- A new method `beginForceAuthOAuthFlow` for FxA `force_auth` for Android and iOS. Force Auth allows you to provide
an email as part of its API, which forces the user to use a specific email during the OAuth flow.
This method is useful when 'reconnecting' after a password change.

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.29.0...master)
