**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.1.0...main)

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
### ⚠️ Breaking Changes ⚠️
- Removed some functions related to sync interruption.  These were never really completed and don't seem to be in use by iOS/Android code:
  - `PlacesApi.new_sync_conn_interrupt_handle()`
  - Swift only: `PlacesAPI.interrupt()`
- The exception variant `InternalPanic` was removed. It's only use was replaced by the already existing `UnexpectedPlacesException`. ([#4847](https://github.com/mozilla/application-services/pull/4847))
### What's New
- The Places component will report more error variants to telemetry. ([#4847](https://github.com/mozilla/application-services/pull/4847))
## Autofill / Logins / Places / Sync Manager, Webext-Storage
### What's Changed
- Updated interruption handling and added support for shutdown-mode which interrupts all operations.

