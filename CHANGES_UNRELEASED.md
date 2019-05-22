**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.28.1...master)

## Places

### What's New

- A new `getRecentBookmarks` API was added to return the list of most recently
  added bookmark items ([#1129](https://github.com/mozilla/application-services/issues/1129)).

### Breaking Changes
- The addition of `getRecentBookmarks` is a breaking change for custom
  implementation of `ReadableBookmarksConnection` on Android
  ([#1129](https://github.com/mozilla/application-services/issues/1129)).

