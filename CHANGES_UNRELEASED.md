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

## Tabs

### What's Changed

The Tabs engine is now more efficient in how it fetches its records:

- The Tabs engine no longer clears the DB on every sync.
- Tabs now tracks the last time it synced and only fetches tabs that have changed since the last sync.
- Tabs will keep records for up to 180 days, in parity with the clients engine. To prevent the DB from getting too large.

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊
  - Added `GleanMetrics.NimbusHealth` metrics for measuring duration of `apply_pending_experiments` and `fetch_experiments`. ([#5344](https://github.com/mozilla/application-services/pull/5344))
