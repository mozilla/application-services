**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.0.0...main)

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

## Tabs

### 🦊 What's Changed 🦊

- The Tabs engine now trims the payload to be under the max the server will accept ([#5376](https://github.com/mozilla/application-services/pull/5376))


## Sync Manager

### 🦊 What's Changed 🦊

- Exposing the Sync Manager component to iOS by addressing the existing naming collisions, adding logic to process the telemetry
  data returned in the component's `sync` function, and adding the component to the iOS megazord ([#5359](https://github.com/mozilla/application-services/pull/5359)).
