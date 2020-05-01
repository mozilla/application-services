**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.58.2...master)

## Viaduct

### Breaking changes

- The `include_cookies` setting is not supported anymore (was `false` by default). ([#3096](https://github.com/mozilla/application-services/pull/3096))

## FxA Client

- Added option boolean argument `ignoreCache` to ignore caching for `getDevices`. ([#3066](https://github.com/mozilla/application-services/pull/3066))

### ⚠️ Breaking changes ⚠️
- iOS: Renamed `fetchDevices(forceRefresh)` to `getDevices(ignoreCache)` to establish parity with Android. ([#3066](https://github.com/mozilla/application-services/pull/3066))
- iOS: Renamed argument of `fetchProfile` from `forceRefresh` to `ignoreCache`. ([#3066](https://github.com/mozilla/application-services/pull/3066))
