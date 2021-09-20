**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v84.0.0...main)

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

## Places, Autofill, Webext-Storage

### What's Changed

- Databases which are detected as being corrupt as they are opened will be deleted and re-created.

## Nimbus

### What's New

- [#4455][1]: For both iOS and Android: extra methods on `Variables` to support orderable items:
  - `getEnum` to coerce strings into Enums.
  - `get*List`, `get*Map` to get lists and maps of all types.
  - Dictionary/Map extensions to map string keys to enum keys, and string values to enum values.
- Nimbus now supports multiple features on each branch. This was added with backward compatibility to ensure support for both schemas. ([#4452](https://github.com/mozilla/application-services/pull/4452))
### ⚠️ Breaking Changes ⚠️

- [#4455][1]: Android only: method `Variables.getVariables(key, transform)`, `transform` changes type
  from `(Variables) -> T` to `(Variables) -> T?`.

[1]: https://github.com/mozilla/application-services/pull/4455
