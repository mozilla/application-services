**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.44.0...master)

## Logins

### Breaking Changes

- Login records with a `httpRealm` attribute will now have their `usernameField` and `passwordField
  properties silently cleared, to help ensure data consistency. ([#2158](https://github.com/mozilla/application-services/pull/2158))
- The Android bindings now collect some basic performance and quality metrics via Glean.
  Applications that submit telemetry via Glean *must* request a data review for these new metrcs
  before upgrading to the latest version of the logins component. See the component README.md
  for more details. ([#2225](https://github.com/mozilla/application-services/pull/2225))

### What's new

- Added invalid character checks from Desktop to `LoginsStorage.ensureValid` and introduced `INVALID_LOGIN_ILLEGAL_FIELD_VALUE` error. ([#2262](https://github.com/mozilla/application-services/pull/2262))

## Sync Manager

### Breaking Changes

- When asked to sync all engines, SyncManager will now sync all engines for which a handle has been set.
  Previously it would sync all known engines, panicking if a handle had not been set for some engine.
  While *technically* a breaking chang, we expect that the new behaviour is almost certainly what
  consuming applications actually want in practice.
