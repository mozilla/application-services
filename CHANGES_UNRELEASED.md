**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.5.0...main)

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

## Autofill

### What's Fixed
  - Fixed syncing of autofill when tombstones exist in the local mirror (#5030)

## Nimbus FML ⛅️🔬🔭🔧

### What's New
  - Added support for breaking up FML files using `includes` and separating into different modules with `imports`.
    ([#5031](https://github.com/mozilla/application-services/pull/5031), [#5022](https://github.com/mozilla/application-services/pull/5022), [#5016](https://github.com/mozilla/application-services/pull/5016), [#5014](https://github.com/mozilla/application-services/pull/5014), [#5007](https://github.com/mozilla/application-services/pull/5007), [#4999](https://github.com/mozilla/application-services/pull/4999), [#4997](https://github.com/mozilla/application-services/pull/4997), [#4976](https://github.com/mozilla/application-services/pull/4976))
    - This is _not_ a breaking change, but should be accompanied by a upgrade to the megazord ([#4099](https://github.com/mozilla/application-services/pull/4099)).
    - This also deprecates some commands in the command line interface ([#5022](https://github.com/mozilla/application-services/pull/5022)). These will be removed in a future release.
    - Related proposal document: [FML: Imports and Includes](https://experimenter.info/fml-imports-and-includes).

## Logins

### What's Changed
  - sqlcipher migrations no longer record metrics (#5017)

## Glean
### What's Changed
  - Updated to Glean v50.1.2

## UniFFI
### What's Changed
  - Updated to UniFFI 0.19.3
