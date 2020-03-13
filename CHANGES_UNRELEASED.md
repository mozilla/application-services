**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.54.1...master)

## Places

### ⚠️ Breaking changes ⚠️

- Android: `PlacesConnection.deletePlace` has been renamed to
  `deleteVisitsFor`, to clarify that it might not actually delete the
  page if it's bookmarked, or has a keyword or tags
  ([#2695](https://github.com/mozilla/application-services/pull/2695)).

### What's fixed

- `history::delete_visits_for` (formerly `delete_place_by_guid`) now correctly
  deletes all visits from a page if it has foreign key references, like
  bookmarks, keywords, or tags. Previously, this would cause a constraint
  violation ([#2695](https://github.com/mozilla/application-services/pull/2695)).
