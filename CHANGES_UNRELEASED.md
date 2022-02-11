**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.0.1...main)

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
## ⛅️🔬🔭 Nimbus SDK

### What's fixed

- Fixes a bug where disabling studies did not disable rollouts. ([#4807](https://github.com/mozilla/application-services/pull/4807))

### ✨ What's New ✨

- A message helper is now available to apps wanting to build a Messaging System on both Android and iOS. Both of these access the variables
  provided by Nimbus, and can have app-specific variables added. This provides two functions:
  - JEXL evaluation ([#4813](https://github.com/mozilla/application-services/pull/4813)) which evaluates boolean expressions.
  - String interpolation ([#4831](https://github.com/mozilla/application-services/pull/4831)) which builds strings with templates at runtime.

## Xcode

- Bumped Xcode version from 13.1.0 -> 13.2.1

## Nimbus FML
### What's fixed
- Fixes a bug where each time the fml is run, the ordering of features in the experimenter json is changed. ([#4819](https://github.com/mozilla/application-services/pull/4819))
