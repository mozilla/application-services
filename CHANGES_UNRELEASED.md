**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.2.0...main)

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

## General
### What's new
- Uniffi was upgraded to 0.18.0. For our consumers, this means there now exported types that used to be internal to `uniffi`. ([#4949](https://github.com/mozilla/application-services/pull/4949)).
  - The types are:
    - `Url` alias for `string`
    - `PlacesTimestamp` alias for`i64`
    - `VisitTransitionSet` alias for `i32`
    - `Guid` alias for `string`
    - `JsonObject` alias for `string`
  - Non of the exposed types conflict with a type in iOS so this is not a breaking change.

## Nimbus ⛅️🔬🔭

### What's new

- Make generation of Experimenter compatible YAML repeatable: fields, variables, features and enum variants are listed alphabetically. ([#4964](https://github.com/mozilla/application-services/pull/4964)).

## Tabs
### What's Changed

- The component has been updated for integration into Firefox iOS ([#4905](https://github.com/mozilla/application-services/pull/4905)).
  - The `DeviceType` naming conflict which prevented `rust-components-swift` from generating Tabs code has been resolved.
  - Errors and the `reset` function have been exposed.
  - Parameters for the `sync` function have been updated to match the `SyncUnlockInfo` parameters.
  - The `tabs-sync` example has been updated with the above changes.
