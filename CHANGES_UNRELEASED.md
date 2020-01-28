**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

<<<<<<< HEAD
[Full Changelog](https://github.com/mozilla/application-services/compare/v0.53.0...master)
=======
[Full Changelog](https://github.com/mozilla/application-services/compare/v0.52.0...master)

## FxA Client

### What's changed

- The `ensureCapabilities` method will not perform any network requests if the
  given capabilities are already registered with the server.
  ([#2681](https://github.com/mozilla/application-services/pull/2681)).

## Libs

### What's changed

- The project now builds with version 4.3.0 of SQL Cipher instead of a fork
  of version 4.2.0. Newest version has NSS crypto backend.
  ([Issue #1386](https://github.com/mozilla/application-services/issues/1386)).
>>>>>>> Set LTO to thin. Fixes #2531. [ci full]
