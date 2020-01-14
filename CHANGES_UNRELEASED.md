**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## FxA Client

### Breaking Changes

* `migrateFromSessionToken` now returns a bool (instead of nothing), True if migration succeeded,
False if migration failed while calling network methods.

### Features

* `migrateFromSessionToken` now handles offline use cases. It caches the data the consumers originally provide.
If there's no network connectivity then the migration could be retried using the new `retryMigration` method.

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.2...master)

