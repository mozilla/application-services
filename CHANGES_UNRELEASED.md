**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.57.0...master)

## General

- Android: Gradle wrapper version upgraded to `6.3`, Android Gradle Plugin version upgraded to `3.6.0`. ([#2917](https://github.com/mozilla/application-services/pull/2917))

## FxA Client

- Added an optional `ttl` parameter to `getAccessToken` to limit the lifetime of the token. ([#2896](https://github.com/mozilla/application-services/pull/2896))
