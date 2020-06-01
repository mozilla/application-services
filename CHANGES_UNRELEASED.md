**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.59.0...master)

## General

- Remove `failure` from the sync_tests and replace it with `anyhow`. ([#3188](https://github.com/mozilla/application-services/pull/3188))

- Adds an alias for generating protobuf files, you can now use `cargo regen-protobufs` to generate them. ([#3178](https://github.com/mozilla/application-services/pull/3178))

- Replaced `failure` with `anyhow` and `thiserror`. ([#3132](https://github.com/mozilla/application-services/pull/3132))

- Android: Added `getTopFrecentSiteInfos` API to retrieve a list of the top frecent sites in `PlacesReaderConnection`. ([#2163](https://github.com/mozilla/application-services/issues/2163))

## FxA Client

### What's new

- Additional special case for China FxA in `getPairingAuthorityURL`. ([#3160](https://github.com/mozilla/application-services/pull/3160))
- Silently ignore push messages for unrecognized commands, rather than reporting an error. ([#3177](https://github.com/mozilla/application-services/pull/3177))
