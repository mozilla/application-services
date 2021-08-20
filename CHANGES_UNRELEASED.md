**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v82.2.0...main)

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

## Android

### ⚠️ Breaking Changes ⚠️
  - Many error classes have been renamed from `FooError` or `FooErrorException` to just `FooException`,
    to be more in keeping with Java/Kotlin idioms.
    - This is due to UniFFi now replacing trailing 'Error' named classes to 'Exception'

## Autofill

### ⚠️ Breaking Changes ⚠️
  - The `Error` enum is now called `AutofillError` (`AutofillException` in Kotlin) to avoid conflicts with builtin names.
