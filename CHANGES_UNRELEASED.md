**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.31.0...master)

## FxA Client

### What's new

- Added a new method to help recover from invalid access tokens. If the application receives an
  an authentication exception while using a token obtained through `FirefoxAccount.getAccessToken`,
  it should:
  - Call `FirefoxAccount.clearAccessTokenCache` to remove the invalid token from the internal cache.
  - Retry the operation after obtaining fresh access token via `FirefoxAccount.getAccessToken`.
  - If the retry also fails with an authentication exception, then the user will need to reconnect
    their account via a fresh OAuth flow.
- `FirefoxAccount.getProfile` now performs the above retry logic automagically. An authentication
  error while calling `getProfile` indicates that the user needs to reconnect their account.

## Logins

### Breaking Changes

- iOS: LoginsStoreError enum variants have their name `lowerCamelCased`
  instead of `UpperCamelCased`, to better fit with common Swift code style.
  ([#1042](https://github.com/mozilla/application-services/issues/1042))
