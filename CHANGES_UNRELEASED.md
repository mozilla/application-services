**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v87.2.0...main)

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
## viaduct
### What's New
- Add support for PATCH methods. ([#4751](https://github.com/mozilla/application-services/pull/4751))

## Nimbus
### What's new
  - The Nimbus SDK now support application version targeting, where experiment creators can set `app_version|versionCompare({VERSION}) >= 0` and the experiments will only target users running `VERSION` or higher. ([#4752](https://github.com/mozilla/application-services/pull/4752))
      - The `versionCompare` transform will return a positive number if `app_version` is greater than
      `VERSION`, a negative number if `app_version` is less than `VERSION` and zero if they are equal
      - `VERSION` must be passed in as a string, for example: `app_version|versionCompare('95.!') >= 0` will target users who are on any version starting with `95` or above (`95.0`, `95.1`, `95.2.3-beta`, `96` etc..)
