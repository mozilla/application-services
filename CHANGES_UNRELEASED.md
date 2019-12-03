**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.44.0...master)

## Logins

### What's new

- Added invalid character checks from Desktop to `LoginsStorage.ensureValid` and introduced `INVALID_LOGIN_ILLEGAL_FIELD_VALUE` error. ([#2262](https://github.com/mozilla/application-services/pull/2262))

## Sync Manager

### Breaking Changes

- When asked to sync all engines, SyncManager will now sync all engines for which a handle has been set.
  Previously it would sync all known engines, panicking if a handle had not been set for some engine.
  While *technically* a breaking chang, we expect that the new behaviour is almost certainly what
  consuming applications actually want in practice.
