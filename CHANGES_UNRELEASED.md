**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.59.0...master)

## General

- Adds an alias for generating protobuf files, you can now use `cargo regen-protobufs` to generate them. ([#3178](https://github.com/mozilla/application-services/pull/3178))

- Replaced `failure` with `anyhow` and `thiserror`. ([#3132](https://github.com/mozilla/application-services/pull/3132))

## FxA Client

### What's new

- Additional special case for China FxA in `getPairingAuthorityURL`. ([#3160](https://github.com/mozilla/application-services/pull/3160))
