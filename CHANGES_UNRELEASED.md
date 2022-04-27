**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.0.2...main)

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

## Nimbus ⛅️🔭🔬

### What's New
  - Added targeting attributes for `language` and `region`, based upon the `locale`. [#4919](https://github.com/mozilla/application-services/pull/4919)
    - This also comes with an update in the JEXL evaluator to handle cases where `region` is not available.

### What's Changed
  - Fixed: A crash was detected by the iOS team, which was traced to `FeatureHolder.swift`. ([#4924](https://github.com/mozilla/application-services/pull/4924))
    - Regression tests added, and FeatureHolder made stateless in both Swift and Kotlin.
