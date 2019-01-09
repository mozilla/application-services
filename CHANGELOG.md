# Unreleased Changes

**TODO: remove before tagging/publishing a release**

# 0.13.0 (_2019-01-09_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.12.1...v0.13.0)

## General

### What's New

- Upgraded openssl to 1.1.1a ([#474](https://github.com/mozilla/application-services/pull/474))

### What's Fixed

- Fixed issue where backtraces were still enabled, causing crashes on some android devices ((#509)[https://github.com/mozilla/application-services/pull/509])
- Fixed some panics that may occur in corrupt databases or unexpected data. ((#488)[https://github.com/mozilla/application-services/pull/488])

## Places

### What's New

N/A

### What's fixed

- Autocomplete no longer returns more results than requested ((#489)[https://github.com/mozilla/application-services/pull/489])

## Logins

### Deprecated or Breaking Changes

- Deprecated the `reset` method, which does not perform any useful action (it clears sync metadata, such as last sync timestamps and the mirror table). Instead, use the new `wipeLocal` method, or delete the database file. ((#497)[https://github.com/mozilla/application-services/pull/497])

### What's New

- Added the `wipeLocal` method for deleting all local state while leaving remote state untouched. ((#497)[https://github.com/mozilla/application-services/pull/497])
- Added `ensureLocked` / `ensureUnlocked` methods which are identical to `lock`/`unlock`, except that they do not throw if the state change would be a no-op (e.g. they do not require that you check `isLocked` first). ((#495)[https://github.com/mozilla/application-services/pull/495])
- Added an overload to `unlock` and `ensureUnlocked` that takes the key as a ByteArray. Note that this is identical to hex-encoding (with lower-case hex characters) the byte array prior to providing it to the string overload. ((#499)[https://github.com/mozilla/application-services/issues/499])

### What's Fixed

- Clarified which exceptions are thrown in documentation in cases where it was unclear. ((#495)[https://github.com/mozilla/application-services/pull/495])
- Added `@Throws` annotations to all methods which can throw. ((#495)[https://github.com/mozilla/application-services/pull/495])
