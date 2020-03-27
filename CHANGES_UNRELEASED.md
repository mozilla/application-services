**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.56.0...master)

## General

### ⚠️ Breaking changes ⚠️

- iOS: The `reqwest` network stack will not be initialized automatically anymore.
Please call `Viaduct.shared.useReqwestBackend()` as soon as possible before using the framework. ([#2880](https://github.com/mozilla/application-services/pull/2880))

## Logins

### What's New

- A new function was added to return a list of duplicate logins, ignoring
  username. ([#2542](https://github.com/mozilla/application-services/pull/2542))
