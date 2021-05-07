**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## Sync Manager

- Removed support for the wipeAll command (#4006)

## Autofill

### What's Changed

- Added support to scrub encrypted data to handle lost/corrupted client keys.
  Scrubbed data will be replaced with remote data on the next sync.

## Nimbus

 - Added bucket and collections to `NimbusServerSettings`, with default values.
 - Added `getAvailableExperiments()` method exposed by `NimbusClient`.
 - At most one local experiment will be enrolled for any given `featureId`, and
  to support this, the database can now have a NotEnrolledReason::FeatureConflict value.

### ⚠️ Breaking changes ⚠️

- Moved the `Nimbus` class and its test class from Android Components into this repository. Existing integrations should pass a delegate in to provide Nimbus with a thread to do I/O and networking on, and an Obsevrer.
  Fixed in the complementary [android-components#10144](https://github.com/mozilla-mobile/android-components/pull/10144)

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.2.0...main)
