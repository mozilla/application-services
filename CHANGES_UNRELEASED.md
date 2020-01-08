**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.0...master)

## General

- Revert NSS to version 3.46.

## Logins

### What's changed

* The error strings returned by `LoginsStorage.importLogins` as part of the migration metrics bundle,
  no longer include potentially-sensitive information such as guids.
