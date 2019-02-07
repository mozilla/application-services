# Unreleased Changes

**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.16.1...master)

## FxA

### What's New

- We are now using [Protocol Buffers](https://developers.google.com/protocol-buffers/) to pass the Profile data across the FFI boundaries, both on Android and iOS. On Android there should be no breaking changes.

### Breaking changes

- iOS: You now have to include the `SwiftProtobuf` framework in your projects for FxAClient to work (otherwise you'll get a runtime error when fetching the user profile). It is built into `Carthage/Build/iOS` just like `FxAClient.framework`.
- iOS: In order to build FxAClient from source, you need [swift-protobuf](https://github.com/apple/swift-protobuf) installed. Simply run `brew install swift-protobuf` if you have Homebrew.
- iOS: You need to run `carthage bootstrap` at the root of the repository at least once before building the FxAClient project: this will build the `SwiftProtobuf.framework` file needed by the project.
- iOS: the `Profile` class now inherits from `RustProtobuf`. Nothing should change in practice for you.

# 0.16.1 (_2019-02-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.16.0...v0.16.1)

## Logins

### What's Fixed

- iOS `LoginRecord`s will no longer use empty strings for `httpRealm` and `formSubmitUrl` in cases where they claim to use nil. ([#623](https://github.com/mozilla/application-services/issues/623))
    - More broadly, all optional strings in LoginRecords were were being represented as empty strings (instead of nil) unintentionally. This is fixed.
- iOS: Errors that were being accidentally swallowed should now be properly reported. ([#640](https://github.com/mozilla/application-services/issues/640))
- Schema initialization/upgrade now happen in a transaction. This should avoid corruption if some unexpected error occurs during the first unlock() call. ([#642](https://github.com/mozilla/application-services/issues/642))

### Breaking changes

- iOS: Code that expects empty strings (and not nil) for optional strings should be updated to check for nil instead. ([#623](https://github.com/mozilla/application-services/issues/623))
    - Note that this went out in a non-major release, as it doesn't cause compilation failure, and manually reading all our dependents determined that nobody was relying on this behavior.

## FxA

### What's Fixed

- iOS: Some errors that were being accidentally swallowed should now be properly reported. ([#640](https://github.com/mozilla/application-services/issues/640))

## Places

### What's New

- New methods on PlacesConnection (Breaking changes for classes implementing PlacesAPI):
    - `fun deleteVisit(url: String, timestamp: Long)`: If a visit exists at the specified timestamp for the specified URL, delete it. This change will be synced if it is the last remaining visit (standard caveat for partial visit deletion). ([#621](https://github.com/mozilla/application-services/issues/621))
    - `fun deleteVisitsBetween(start: Long, end: Long)`: Similar to `deleteVisitsSince(start)`, but takes an end date. ([#621](https://github.com/mozilla/application-services/issues/621))
    - `fun getVisitInfos(start: Long, end: Long = Long.MAX_VALUE): List<VisitInfo>`: Returns a more detailed set of information about the visits that occured. ([#619](https://github.com/mozilla/application-services/issues/619))
        - `VisitInfo` is a new data class that contains a visit's url, title, timestamp, and type.

### Breaking Changes

- The new `PlacesConnection` methods listed in the "What's New" all need to be implemented (or stubbed) by any class that implements `PlacesAPI`. (multiple bugs, see "What's New" for specifics).

### What's fixed

- Locally deleted visits deleted using `deleteVisitsSince` should not be resurrected on future syncs. ([#621](https://github.com/mozilla/application-services/issues/621))

# 0.16.0 (_2019-02-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.15.0...v0.16.0)

## General

### What's New

- iOS builds now target v11.0. ([#614](https://github.com/mozilla/application-services/pull/614))
- Preparatory infrastructure for megazording iOS builds has landed.([#625](https://github.com/mozilla/application-services/pull/625))

## Places

### Breaking Changes

- Several new methods on PlacesConnection (Breaking changes for classes implementing PlacesAPI):
    -  `fun interrupt()`. Cancels any calls to `queryAutocomplete` or `matchUrl` that are running on other threads. Those threads will throw an `OperationInterrupted` exception. ([#597](https://github.com/mozilla/application-services/pull/597))
        - Note: Using `interrupt()` during the execution of other methods may work, but will have mixed results (it will work if we're currently executing a SQL query, and not if we're running rust code). This limitation may be lifted in the future.
    - `fun deletePlace(url: String)`: Deletes all visits associated with the provided URL ([#591](https://github.com/mozilla/application-services/pull/591))
        - Note that these deletions are synced!
    - `fun deleteVisitsSince(since: Long)`: Deletes all visits between the given unix timestamp (in milliseconds) and the present ([#591](https://github.com/mozilla/application-services/pull/591)).
        - Note that these deletions are synced!

### What's New

- Initial support for storing bookmarks has been added, but is not yet exposed over the FFI. ([#525](https://github.com/mozilla/application-services/pull/525))

## FxA

### What's Fixed

- iOS Framework: Members of Avatar struct are now public. ([#615](https://github.com/mozilla/application-services/pull/615))


# 0.15.0 (_2019-02-01_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.14.0...v0.15.0)

## General

### What's New

- A new megazord was added, named `fenix-megazord`. It contains the components for FxA and Places (and logging). ([#585](https://github.com/mozilla/application-services/issues/585))
    - Note: To use this, you must be on version 0.3.1 of the gradle plugin.

## Logins

### What's Fixed

- Fix an issue where unexpected errors would become panics. ([#593](https://github.com/mozilla/application-services/pull/593))
- Fix an issue where syncing with invalid credentials would be reported as the wrong kind of error (and cause a panic because of the previous issue). ([#593](https://github.com/mozilla/application-services/pull/593))

## Places

### What's New

- New method on PlacesConnection (breaking change for classes implementing PlacesAPI): `fun matchUrl(query: String): String?`. This is similar to `queryAutocomplete`, but only searches for URL and Origin matches, and only returns (a portion of) the matching url (if found), or null (if not). ([#595](https://github.com/mozilla/application-services/pull/595))

### What's Fixed

- Autocomplete will no longer return an error when asked to match a unicode string. ([#298](https://github.com/mozilla/application-services/issues/298))

- Autocomplete is now much faster for non-matching queries and queries that look like URLs. ([#589](https://github.com/mozilla/application-services/issues/589))

## FxA

### What's New

- It is now possible to know whether a profile avatar has been set by the user. ([#579](https://github.com/mozilla/application-services/pull/579))

### Breaking Changes

- The `avatar` accessor from the `Profile` class in the Swift framework now returns an optional `Avatar` struct instead of a `String`. ([#579](https://github.com/mozilla/application-services/pull/579))

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
