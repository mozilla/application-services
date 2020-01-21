**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.2...master)

## Places

- The Dogear library for merging synced bookmarks has been updated to the latest version.
  ([#2469](https://github.com/mozilla/application-services/pull/2469))
- Places now exposes `resetHistorySyncMetadata` and `resetBookmarkSyncMetadata`
  methods, which cleans up all Sync state, including tracking flags and change
  counters. These methods should be called by consumers when the user signs out,
  to avoid tracking changes and causing unexpected behavior the next time they
  sign in.
  ([#2447](https://github.com/mozilla/application-services/pull/2447))

### Breaking Changes

- The Android bindings now collect some basic performance and quality metrics via Glean.
  Applications that submit telemetry via Glean must request a data review for these metrics
  before integrating the places component. See the component README.md for more details.
  ([#2431](https://github.com/mozilla/application-services/pull/2431))
