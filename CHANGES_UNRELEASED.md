**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.0.0...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's Changed
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's New
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## Places

### What's Changed
  - The database initialization code now uses BEGIN IMMIDIATE to start a
    transaction.  This will hopefully prevent `database is locked` errors when
    opening a sync connection.

### What's New

  - The `HistoryVisitInfo` struct now has an `is_remote` boolean which indicates whether the
    represented visit happened locally or remotely. ([#4810](https://github.com/mozilla/application-services/pull/4810))
