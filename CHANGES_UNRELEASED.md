**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.4.0...main)

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

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊
- Updated the Nimbus Gradle Plugin to fix a number of issues after migrating it to this repository ([#5348](https://github.com/mozilla/application-services/pull/5348))
- Good fences: protected calls out to the error reporter with a `try`/`catch` ([#5366](https://github.com/mozilla/application-services/pull/5366))

## FxA Client
### 🦊 What's Changed 🦊
  - The FxA Client now attempts to merge values from any old persisted state before writing the new state to persisted storage.([#0000](https://github.com/mozilla/application-services/pull/0000))
     - Currently only merges the `last_handled_command` index, to ensure the state always reflects the highest last handled command.
     - This is mostly relevant for iOS, since in iOS push notifications are managed
    in a separate process. The main process could then overwrite the persisted state
    updated by the notification process, causing the main process to retrieve tabs the
    notification process already retrieved.
