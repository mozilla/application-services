**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.57.0...master)

## General

- Android: Gradle wrapper version upgraded to `6.3`, Android Gradle Plugin version upgraded to `3.6.0`. ([#2917](https://github.com/mozilla/application-services/pull/2917))
- Android: Upgraded NDK from r20 to r21. ([#2985](https://github.com/mozilla/application-services/pull/2985))
- iOS: Xcode version changed to 11.4.1 from 11.4.0. ([#2996](https://github.com/mozilla/application-services/pull/2996))

## FxA Client

- iOS: `refreshProfile` now takes an optional boolean argument `forceRefresh` to force a network request to be made in every case ([#3000](https://github.com/mozilla/application-services/pull/3000))
- Added an optional `ttl` parameter to `getAccessToken` to limit the lifetime of the token. ([#2896](https://github.com/mozilla/application-services/pull/2896))
