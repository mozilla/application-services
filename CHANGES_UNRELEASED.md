**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.32.2...master)

## FxAClient

### Breaking Changes
	
- iOS: FirefoxAccountError enum variants have their name `lowerCamelCased`
  instead of `UpperCamelCased`, to better fit with common Swift code style.
  ([#1324](https://github.com/mozilla/application-services/issues/1324))
