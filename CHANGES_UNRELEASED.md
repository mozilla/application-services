**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v73.0.0...main)

# Android

- The `-forUnitTest` build no longer includes code compiled for Windows, meaning that
  it is no longer possible to run appservices Kotlin unit tests on Windows. We hope
  this will be a temporary measure while we resolve some build issues.
