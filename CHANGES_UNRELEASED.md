**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.3.0...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### 🦊 What's Changed 🦊
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### ✨ What's New ✨
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## FxA Client
### ✨ What's New ✨
  - Exposes a new API `refreshProfile` that triggers a callback instead of returning a new profile. ([#5333](https://github.com/mozilla/application-services/pull/5333))
     - The `refreshProfile` function will first trigger the callback with the result from cache if one exists,
     - Then, the `refreshProfile` might trigger the callback again if it triggered a network request.
  - For iOS, the behavior should be unchanged, as the `getProfile` function that `refreshProfile` replaces, is only used internally within Application Services.

## Tabs

### What's Changed

The Tabs engine is now more efficient in how it fetches its records:

- The Tabs engine no longer clears the DB on every sync.
- Tabs now tracks the last time it synced and only fetches tabs that have changed since the last sync.
- Tabs will keep records for up to 180 days, in parity with the clients engine. To prevent the DB from getting too large.
