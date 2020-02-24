**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.52.0...master)

## FxA Client

### What's changed

- The `ensureCapabilities` method will not perform any network requests if the
  given capabilities are already registered with the server.
  ([#2681](https://github.com/mozilla/application-services/pull/2681)).

## Places

### What's fixed

- `storage::history::apply_observation` and `storage::bookmarks::update_bookmark`
  now flush pending origin and frecency updates. This fixes a bug where origins
  might be flushed at surprising times, like right after clearing history
  ([#2693](https://github.com/mozilla/application-services/issues/2693)).
