**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.56.0...master)

## General

### ⚠️ Breaking changes ⚠️

- iOS: The `reqwest` network stack will not be initialized automatically anymore.
Please call `Viaduct.shared.useReqwestBackend()` as soon as possible before using the framework. ([#2880](https://github.com/mozilla/application-services/pull/2880))
