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

- JEXL is now available for evaluation from application code in Swift and Android ([#4813](https://github.com/mozilla/application-services/pull/4813)).
    This is the next piece of the puzzle for supporting Messaging Experiments.
