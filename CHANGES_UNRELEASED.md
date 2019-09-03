**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.38.2...master)

## Places

### What's fixed

- Hidden URLs (redirect sources, or links visited in frames) are no longer
  synced or returned in `get_visit_infos` or `get_visit_page`. Additionally,
  a new `is_hidden` flag is added to `HistoryVisitInfo`, though it's currently
  always `false`, since those visits are excluded.
  ([#1715](https://github.com/mozilla/application-services/pull/1715))
