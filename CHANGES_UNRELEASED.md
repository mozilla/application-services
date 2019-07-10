**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.33.2...master)

## General

- All of our cryptographic primitives are now backed by NSS. This change should be transparent our customers.  
If you build application-services, it is recommended to delete the `libs/{desktop, ios, android}` folders and start over using `./build-all.sh [android|desktop|ios]`. [GYP](https://github.com/mogemimi/pomdog/wiki/How-to-Install-GYP) and [ninja](https://github.com/ninja-build/ninja/wiki/Pre-built-Ninja-packages) are required to build these libraries.

## Places

### What's New

- Added `WritableHistoryConnection.acceptResult(searchString, url)` for marking
  an awesomebar result as accepted.
  ([#1332](https://github.com/mozilla/application-services/pull/1332))
    - Specifically, `queryAutocomplete` calls for searches that contain
      frequently accepted results are more highly ranked.

### Breaking changes

- Android only: The addition of `acceptResult` to `WritableHistoryConnection` is
  a breaking change for any custom implementations of `WritableHistoryConnection`
  ([#1332](https://github.com/mozilla/application-services/pull/1332))

## Push

### Breaking Changes

- `OpenSSLError` has been renamed to the more general `CryptoError`.
