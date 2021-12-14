**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v87.1.0...main)

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

### ✨✨ What's New ✨✨

#### ⛅️🔭🔬 Nimbus

- Initial release of the Nimbus Feature Manifest Language tool (`nimbus-fml`).
  - This is a significant upgrade to the Variables API, adding code-generation to Kotlin and Experimenter compatible manifest JSON.
  - [RFC for language specification](https://github.com/mozilla/experimenter-docs/pull/156).
  - This is the first release it is made available to client app's build processes.
  - [Build on CI](https://github.com/mozilla/application-services/pull/4701) ready for application build processes to download.
