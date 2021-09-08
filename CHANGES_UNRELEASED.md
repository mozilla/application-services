**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v82.3.0...main)

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

## Push
### ⚠️ Breaking Changes ⚠️
- The `unsubscribeAll` API will now return nothing as opposed to a boolean. The boolean was misleading as it only ever returned true.
  errors can be caught using exceptions. ([#4418](https://github.com/mozilla/application-services/pull/4418))
### What's Changed
 - The push `unsubscribe` API will no longer accept a null `channel_id` value, a valid `channel_id` must be presented, otherwise rust will panic and an error will be thrown to android.
  note that this is not a breaking change, since our hand-written Kotlin code already ensures that the function can only be called with a valid, non-empty, non-nullable string. ([#4402](https://github.com/mozilla/application-services/pull/4402))

## Nimbus

### What's Changed

- `DatabaseNotReady` exceptions are no longer reported to the error reporter on either Android or iOS. [#4438](https://github.com/mozilla/application-services/pull/4438)
- `NimbusErrorException` has been renamed `NimbusException`. This internal API, so is not a breaking change.