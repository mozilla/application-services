**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## Nimbus ☁️🔬

### What's New

 - Android gains a `nimbus.getVariables(featureId: String)` and a new wrapper around JSON data coming straight from Remote Settings.
 - Application features can only have a maximum of one experiment running at a time.

### What's Changed

 - Android and iOS `Branch` objects no longer have access to a `FeatureConfig` object.

[Full Changelog](https://github.com/mozilla/application-services/compare/v76.0.0...main)
