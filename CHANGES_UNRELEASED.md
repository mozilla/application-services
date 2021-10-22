**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v86.1.0...main)

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

## Push
### What's Changed
  - We've changed the database schema to avoid confusion about the state of subscriptions and
    in particular, avoid `SQL: UNIQUE constraint failed: push_record.channel_id` errors
    reported in [#4575](https://github.com/mozilla/application-services/issues/4575). This is
    technically a breaking change as a dictionary described in the UDL changed, but in practice,
    none of our consumers used it, so we are not declaring it as breaking in this context.
