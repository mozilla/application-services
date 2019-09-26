**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.39.4...master)

## Logins

### Breaking Changes

- getHandle has been moved to the LoginsStorage interface. All implementers other than DatabaseLoginsStorage should implement this by throwing a `UnsupportedOperationException`.
