**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.1...master)

## FxA Client

### What's changed

* Fixed a bug in deserializing FxA objects from JSON when the new `introspection_endpoint`
  field is not present.
