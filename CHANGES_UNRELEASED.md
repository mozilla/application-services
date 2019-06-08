**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.32.0...master)

## FxA Client

### Fixes

- Fixes SendTab initializeDevice in Android to use the proper device type ([#1314](https://github.com/mozilla/application-services/pull/1314))

## iOS Bindings

### What's Fixed

- Errors emitted from the rust code should now all properly output their description. ([#1323](https://github.com/mozilla/application-services/pull/1323))

## Logins

### What's Fixed

- Remote login records which cannot be parsed are now ignored (and reported in telemetry). [#1253](https://github.com/mozilla/application-services/issues/1253)

