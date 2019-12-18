**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.46.0...master)

## General

- Updated NSS to version 3.48. ([#2379](https://github.com/mozilla/application-services/issues/2379))

## Logins

### Breaking Changes

- `LoginsStorage.getByHostname` has been removed ([#2152](https://github.com/mozilla/application-services/issues/2152))

### What's new

- `LoginsStorage.getByBaseDomain` has been added ([#2152](https://github.com/mozilla/application-services/issues/2152))
- Removed hard deletion of `SyncStatus::New` records in `delete` and `wipe` logins database functions. ([#2362](https://github.com/mozilla/application-services/pull/2362))
- Android: The `MemoryLoginsStorage` class has been deprecated, because it behaviour has already started to
  diverge from that of `DatabaseLoginStorage`. To replace previous uses of this class in tests, please either
  explicitly mock the `LoginsStorage` interface or use a `DatabaseLoginStorage` with a tempfile or `":memory:"`
  as the `dbPath` argument.
