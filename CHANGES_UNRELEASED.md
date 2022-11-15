**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v95.0.1...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's Changed
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### ✨ What's New ✨
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## ⛅️🔬🔭 Nimbus

### ✨ What's New ✨
  - `active_experiments` is available to JEXL as a set containing slugs of all enrolled experiments ([#5227](https://github.com/mozilla/application-services/pull/5227))
  - Added query method for behavioral targeting event store ([#5226](https://github.com/mozilla/application-services/pull/5226))

### ⚠️ Breaking Changes ⚠️
  - Changed the type of `customTargetingAttributes` in `NimbusAppSettings` to a `JSONObject`. The change will be breaking only for Android. ([#5229](https://github.com/mozilla/application-services/pull/5229))

## Logins

### What's Changed
  - Include a redacted version of the URL in the Sentry error report when we see a login with an invalid origin field.
  - Made it so `InvalidDatabaseFile` errors aren't reported to Sentry.  These occurs when a non-existent path is passed
    to `migrateLoginsWithMetrics()`, which happens about 1-2 times a day.  This is very low volume, the code is going
    away soon, and we have a plausible theory that these happen when Fenix is killed after the migration but before
    `SQL_CIPHER_MIGRATION` is stored.

## Places

### What's Changed
  - Report a Sentry breadcrumb when we fail to parse URLs, with a redacted version of the URL.

## JwCrypto

### What's Changed
  - Log a breadcrumb with a redacted version of the crypto key when it has an invalid form (before throwing
    DeserializationError)

## FxA Client
### What's changed
  - The `processRawIncomingAccountEvent` function will now process all commands, not just one. This moves the responsibilty of ensuring each push gets a UI element to the caller.
