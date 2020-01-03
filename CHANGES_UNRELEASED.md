**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## Logins

### Breaking Changes

- `LoginsStorage.importLogins` returns logins migration metrics as JSON object. ([#2382](https://github.com/mozilla/application-services/issues/2382))

- iOS only: Added a migration path for apps to convert the encrypted database headers to plaintext([#2100](https://github.com/mozilla/application-services/issues/2100)).  
New databases must be opened using `LoginsStorage.unlockWithKeyAndSalt` instead of `LoginsStorage.unlock` which is now deprecated.  
To migrate current users databases, it is required to call `LoginsStorage.migrateToPlaintextHeader` before opening the database. This new method requires a salt. The salt persistence is now the responsibility of the application, which should be stored alongside the encryption key. For an existing database, the salt can be obtained using `LoginsStorage.getDbSaltForKey`.

### What's new

- Android: Added ability to rekey the database via `rekeyDatabase`. [[#2228](https://github.com/mozilla/application-services/pull/2228)]

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.43.1...master)
## FxA Client

### Breaking Changes

* Android: `migrateFromSessionToken` now reuses the existing 'sessionToken' instead of creating a new session token.

### What's new

* Android: New method `copyFromSessionToken` will create a new 'sessionToken' state, this is what `migrateFromSessionToken` used to do,
before this release.

## Places

### Breaking Changes

- - `PlacesApi.importVisitsFromFennec` return history migration metrics as JSON object. ([#2414](https://github.com/mozilla/application-services/issues/2414))

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.44.0...master)
