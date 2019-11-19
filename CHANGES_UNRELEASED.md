**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.45.1...master)

## Logins

### Breaking Changes

- The Android bindings now collect some basic performance and quality metrics via Glean.
  Applications that submit telemetry via Glean must request a data review for these metrics
  before integrating the logins component. See the component README.md for more details.
  ([#2225](https://github.com/mozilla/application-services/pull/2225))
- `username`, `usernameField`, and `passwordField` are no longer
  serialized as `null` in the case where they are empty strings. ([#2252](https://github.com/mozilla/application-services/pull/2252))
  - Android: `ServerPassword` fields `username`, `usernameField`, and
    `passwordField` are now required fields -- `null` is not acceptable,
    but empty strings are OK.
  - iOS: `LoginRecord` fields `username`, `usernameField` and
    `passwordField` are no longer nullable.
