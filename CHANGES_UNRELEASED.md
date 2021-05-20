**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## Nimbus ☁️🔬

### What's New

 - Android gains a `nimbus.getVariables(featureId: String)` and a new wrapper around JSON data coming straight from Remote Settings.
 - Application features can only have a maximum of one experiment running at a time.

### What's Changed

 - Android and iOS `Branch` objects no longer have access to a `FeatureConfig` object.

### ⚠️ Breaking changes ⚠️
- The experiment database will be migrating from version 1 to version 2 on
  first run.  *Various kinds of incorrectly specified feature and featureId
  related fields will be detected, and any related experiments & enrollments
  will be discarded.  Experiments & enrollments will also be discarded if they
  are missing other required fields (eg schemaVersion).  If there is an error
  during the database upgrade, the database will be wiped, since losing
  existing enrollments is still less bad than having the database in an unknown
  inconsistent state.*

[Full Changelog](https://github.com/mozilla/application-services/compare/v76.0.1...main)
