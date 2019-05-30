**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## FxA Client

Features

* Added `migrateFromSessionToken` to allow creating a refreshToken from an existing sessionToken. 
Useful for Fennec to Fenix bootstrap flow, where the user can just reuse the existing sessionToken to 
create a new session with a refreshToken.

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.29.0...master)
