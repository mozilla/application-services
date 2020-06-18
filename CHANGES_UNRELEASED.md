**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.3...master)

* Fix an issue where a node reassignment or signing out and signing back in
  wouldn't clear the locally stored last sync time for engines
  ([#3150](https://github.com/mozilla/application-services/issues/3150),
  PR [#3241](https://github.com/mozilla/application-services/pull/3241)).
