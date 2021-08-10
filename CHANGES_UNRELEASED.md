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

## Logins

### ⚠️ Breaking Changes ⚠️
  - Members of the `LoginsStoreError` enum no longer have a `message` property giving a detailed error message.
    It is now just a plain enum with no associated data.

## Autofill

### ⚠️ Breaking Changes ⚠️
  - The `Error` enum is now called `AutofillError` (`AutofillException` in Kotlin) to avoid conflicts with
    builtin names.

## Push

### What's changed
  - The push component will now attempt to auto-recover from the server losing its UAID ([#4347](https://github.com/mozilla/application-services/pull/4347))
    - The push component will return a new kotlin Error `UAIDNotRecognizedError` in cases where auto-recovering isn't possible (when subscribing)
    - Two other new errors were defined that were used to be reported under a generic error:
      - `JSONDeserializeError` for errors in deserialization
      - `RequestError` for errors in sending a network request

## Nimbus
### What's changed
   - Nimbus on iOS will now post a notification when it's done fetching experiments, to match what it does when applying experiments. ([#4378](https://github.com/mozilla/application-services/pull/4378))
