**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.53.0...master)

## Android

### What's changed

- A megazord loading failure will throw as soon as possible rather than at call time.
  ([#2739](https://github.com/mozilla/application-services/issues/2739))

## iOS

### What's New

- Developers can now run `./libs/verify-ios-environment.sh` to ensure their machine is ready to build the iOS Xcode project smoothly. ([#2737](https://github.com/mozilla/application-services/pull/2737))

## FxA Client

### What's new

- Added `FxAConfig.china` helper function to use FxA/Sync chinese servers. ([#2736](https://github.com/mozilla/application-services/issues/2736))
