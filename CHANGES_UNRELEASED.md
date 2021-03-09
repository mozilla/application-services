**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## FxA Client

### ⚠️ Breaking changes ⚠️

- The Kotlin and Swift bindings are now generated automatically using UniFFI.
  As a result many small details of the API surface have changed, such as some
  classes changing names to be consistent between Rust, Kotlin and Swift.
  ([#3876](https://github.com/mozilla/application-services/pull/3876))

[Full Changelog](https://github.com/mozilla/application-services/compare/v72.1.0...main)

- The top-level Rust workspace now builds with Rust 1.50
- The top-level Rust workspace is now stapled to Rust 1.50, so all developers
  will build with 1.50, as will the continuous integration for this repo.
