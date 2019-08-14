**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.38.0...master)

## Android

### What's new

- Initial versions of Fennec data import methods have landed:
  - Bookmarks and history visits can be imported by calling `PlacesWriterConnection.importBookmarksFromFennec` and `PlacesWriterConnection.importVisitsFromFennec` respectively.
  - Logins can be imported with `LoginsStorage.importLogins`.
