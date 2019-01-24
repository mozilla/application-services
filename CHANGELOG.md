# Unreleased Changes

**See [the release process docs](docs/release-process.md) for the steps to take when cutting a new release.**

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.14.0...master)

## Places

### What's Fixed

- Autocomplete will no longer return an error when asked to match a unicode string. ([#298](https://github.com/mozilla/application-services/issues/298))

# 0.14.0 (_2019-01-23_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.13.3...v0.14.0)

## General

### What's New

- A new component was added for customizing how our Rust logging is handled. It allows Android code to get a callback whenever a log is emitted from Rust (Most users will not need to use this directly, but instead will consume it via the forthcoming helper that hooks it directly into android-components Log system in [android-components PR #1765](https://github.com/mozilla-mobile/android-components/pull/1765)). ([#472](https://github.com/mozilla/application-services/pull/472))

- The gradle megazord plugin updated to version 0.3.0, in support of the logging library. Please update when you update your version of android-components. ([#472](https://github.com/mozilla/application-services/pull/472))

- In most cases, opaque integer handles are now used to pass data over the FFI ([#567](https://github.com/mozilla/application-services/issues/567)). This should be more robust, and allow detection of many types of errors that would previously cause silent memory corruption.

  This should be mostly transparent, but is a semi-breaking semantic change in the case that something throws an exception indicating that the Rust code paniced (which should only occur due to bugs anyway). If this occurs, all subsequent operations on that object (except `close`/`lock`) will cause errors. It is "poisoned", in Rust terminology. (In the future, this may be handled automatically)

  This may seem inconvenient, but it should be an improvement over the previous version, where we instead would simply carry on despite potentially having corrupted internal state.

- Build settings were changed to reduce binary size of Android `.so` by around 200kB (per library). ([#567](https://github.com/mozilla/application-services/issues/567))

- Rust was updated to 1.32.0, which means we no longer use jemalloc as our allocator. This should reduce binary size some, but at the cost of some performance. (No bug as this happens automatically as part of CI, see the rust-lang [release notes](https://blog.rust-lang.org/2019/01/17/Rust-1.32.0.html#jemalloc-is-removed-by-default) for more details).

### Breaking Changes

- Megazord builds will no longer log anything by default. Logging must be enabled as described "What's New". ([#472](https://github.com/mozilla/application-services/pull/472))

## Places

### What's Fixed

- PlacesConnection.getVisited will now return that invalid URLs have not been visited, instead of throwing. ([#552](https://github.com/mozilla/application-services/issues/552))
- PlacesConnection.noteObservation will correctly identify url parse failures as such. ([#571](https://github.com/mozilla/application-services/issues/571))
- PlacesConnections not utilizing encryption will not make calls to mlock/munlock on every allocation/free. This improves performance up to 6x on some machines. ([#563](https://github.com/mozilla/application-services/pull/563))
- PlacesConnections now use WAL mode. ([#555](https://github.com/mozilla/application-services/pull/563))

## FxA

### Breaking Changes

Some APIs which are semantically internal (but exposed for various reasons) have changed.

- Android: Some `protected` methods on `org.mozilla.fxaclient.internal.RustObject` have been changed (`destroy` now takes a `Long`, as it is an opaque integer handle). This object should not be considered part of the public API of FxA, but it is still available. Users using it are recommended not to do so. ([#567](https://github.com/mozilla/application-services/issues/567))
- iOS: The type `RustOpaquePointer` was replaced by `RustHandle`, which is a `RustPointer<UInt64>`. While these are technically part of the public API, they may be removed in the future and users are discouraged from using them. ([#567](https://github.com/mozilla/application-services/issues/567))

# 0.13.3 (_2019-01-11_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.13.2...v0.13.3)

## Places

### What's Fixed

- Places will no longer log PII. ([#540](https://github.com/mozilla/application-services/pull/540))

# 0.13.2 (_2019-01-11_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.13.1...v0.13.2)

## Firefox Accounts

### What's New

- The fxa-client android library will now write logs to logcat. ([#533](https://github.com/mozilla/application-services/pull/533))
- The fxa-client Android and iOS librairies will throw a differentiated exception for general network errors. ([#535](https://github.com/mozilla/application-services/pull/535))

# 0.13.1 (_2019-01-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.13.0...v0.13.1)

Note: This is a patch release that works around a bug introduced by a dependency. No functionality has been changed.

## General

### What's New

N/A

### What's Fixed

- Network requests on Android. Due to a [bug in `reqwest`](https://github.com/seanmonstar/reqwest/issues/427), it's version has been pinned until we can resolve this issue. ([#530](https://github.com/mozilla/application-services/pull/530))

# 0.13.0 (_2019-01-09_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.12.1...v0.13.0)

## General

### What's New

- Upgraded openssl to 1.1.1a ([#474](https://github.com/mozilla/application-services/pull/474))

### What's Fixed

- Fixed issue where backtraces were still enabled, causing crashes on some android devices ([#509](https://github.com/mozilla/application-services/pull/509))
- Fixed some panics that may occur in corrupt databases or unexpected data. ([#488](https://github.com/mozilla/application-services/pull/488))

## Places

### What's New

N/A

### What's fixed

- Autocomplete no longer returns more results than requested ([#489](https://github.com/mozilla/application-services/pull/489))

## Logins

### Deprecated or Breaking Changes

- Deprecated the `reset` method, which does not perform any useful action (it clears sync metadata, such as last sync timestamps and the mirror table). Instead, use the new `wipeLocal` method, or delete the database file. ([#497](https://github.com/mozilla/application-services/pull/497))

### What's New

- Added the `wipeLocal` method for deleting all local state while leaving remote state untouched. ([#497](https://github.com/mozilla/application-services/pull/497))
- Added `ensureLocked` / `ensureUnlocked` methods which are identical to `lock`/`unlock`, except that they do not throw if the state change would be a no-op (e.g. they do not require that you check `isLocked` first). ([#495](https://github.com/mozilla/application-services/pull/495))
- Added an overload to `unlock` and `ensureUnlocked` that takes the key as a ByteArray. Note that this is identical to hex-encoding (with lower-case hex characters) the byte array prior to providing it to the string overload. ([#499](https://github.com/mozilla/application-services/issues/499))

### What's Fixed

- Clarified which exceptions are thrown in documentation in cases where it was unclear. ([#495](https://github.com/mozilla/application-services/pull/495))
- Added `@Throws` annotations to all methods which can throw. ([#495](https://github.com/mozilla/application-services/pull/495))
