**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.1.0...main)

## Autofill

### What's Changed

- `get_address()` and `get_credit_card()` now throw a NoSuchRecord error instead of SqlError when the GUID is not found
