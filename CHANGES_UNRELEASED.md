**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.44.0...master)

## Logins

### What's new

- Added invalid character checks from Desktop to `LoginsStorage.ensureValid` and introduced `INVALID_LOGIN_ILLEGAL_FIELD_VALUE` error. ([#2262](https://github.com/mozilla/application-services/pull/2262))
