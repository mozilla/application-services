**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.55.0...master)

## FxA

### ⚠️ Breaking changes ⚠️

- iOS: `migrateFromSessionToken` and `retryMigrateFromSessionToken` no longer return a boolean.
Instead they return a `MigrationData?`, if migrated successfully the structure returns `{ total_duration }`,
where `total_duration` is the time the migration took in milliseconds. ([#2824](https://github.com/mozilla/application-services/pull/2824)).

## Libs

### What's changed

- The project now builds with version 4.3.0 of SQLCipher instead of a fork
  of version 4.2.0. Newest version has NSS crypto backend. ([#2822](https://github.com/mozilla/application-services/pull/2822)).

## FxA Client

### Breaking changes

- `Server.dev` is now `Server.stage` to reflect better the FxA server instance it points to. ([#2830](https://github.com/mozilla/application-services/pull/2830)).
