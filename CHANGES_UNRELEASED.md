**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.45.1...master)

## Logins

### Breaking Changes

- `username`, `usernameField`, and `passwordField` are no longer
  serialized as `null` in the case where they are empty strings.
