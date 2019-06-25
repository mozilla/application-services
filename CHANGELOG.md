# v0.32.0 (_2019-06-14_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.31.2...v0.32.0)

## Places

### What's fixed

- Fix an error that could happen when the place database is closed.
  ([#1304](https://github.com/mozilla/application-services/pull/1304))

- iOS only: Ensure interruption errors don't come through as network errors.
  ([#1304](https://github.com/mozilla/application-services/pull/1304))

# v0.31.2 (_2019-06-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.31.1...v0.31.2)

## Sync

### What's fixed

- Fixes an edge case introduced in v0.31.1 where a users set of declined engines
  (aka the "Choose what to Sync" preferences) could be forgotten.
  ([#1273](https://github.com/mozilla/application-services/pull/1273))

# v0.31.1 (_2019-06-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.31.0...v0.31.1)

## Sync

### What's fixed

- Fixes an issue where a stale sync key will be used in cases where a user signs
  out and signs in to another account. ([#1256](https://github.com/mozilla/application-services/pull/1256))

## FxA Client

### What's new

- Added a new method to help recover from invalid access tokens.
  ([#1244](https://github.com/mozilla/application-services/pull/1244)) If the
  application receives an an authentication exception while using a token
  obtained through `FirefoxAccount.getAccessToken`, it should:
  - Call `FirefoxAccount.clearAccessTokenCache` to remove the invalid token from the internal cache.
  - Retry the operation after obtaining fresh access token via `FirefoxAccount.getAccessToken`.
  - If the retry also fails with an authentication exception, then the user will need to reconnect
    their account via a fresh OAuth flow.
- `FirefoxAccount.getProfile` now performs the above retry logic automagically.
  An authentication error while calling `getProfile` indicates that the user
  needs to reconnect their account.
  ([#1244](https://github.com/mozilla/application-services/pull/1244)

# v0.31.0 (_2019-06-07_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.30.0...v0.31.0)

## Sync

- Android: A new `sync15` package defines Kotlin data classes for the Sync
  telemetry ping. ([#1112](https://github.com/mozilla/application-services/pull/1112))
- Android: `PlacesApi.syncHistory` and `PlacesApi.syncBookmarks` now return a
  `SyncTelemetryPing`. ([#1112](https://github.com/mozilla/application-services/pull/1112))
- iOS: `PlacesAPI.syncBookmarks` now returns a JSON string with the contents of
  the Sync ping. This should be posted to the legacy telemetry submission
  endpoint. ([#1112](https://github.com/mozilla/application-services/pull/1112))

## Places

### What's fixed

- Deduping synced bookmarks with newer server timestamps no longer throws
  unique constraint violations. ([#1259](https://github.com/mozilla/application-services/pull/1259))

## Logins

### Breaking Changes

- iOS: LoginsStoreError enum variants have their name `lowerCamelCased`
  instead of `UpperCamelCased`, to better fit with common Swift code style.
  ([#1042](https://github.com/mozilla/application-services/issues/1042))

# v0.30.0 (_2019-05-30_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.29.0...v0.30.0)

## Push

### Breaking Changes

* Changed the internal serialization format of the Push Keys.

## FxA Client

### Breaking Changes

* Changed the internal serialization format of the Send Tab Keys. Calling `ensureCapabilities` will re-generate them.

### Features

* Added `migrateFromSessionToken` to allow creating a refreshToken from an existing sessionToken.
Useful for Fennec to Fenix bootstrap flow, where the user can just reuse the existing sessionToken to 
create a new session with a refreshToken.

# v0.29.0 (_2019-05-23_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.28.1...v0.29.0)

## Places

### What's New

- A new `getRecentBookmarks` API was added to return the list of most recently
  added bookmark items ([#1129](https://github.com/mozilla/application-services/issues/1129)).

### Breaking Changes
- The addition of `getRecentBookmarks` is a breaking change for custom
  implementation of `ReadableBookmarksConnection` on Android
  ([#1129](https://github.com/mozilla/application-services/issues/1129)).

# v0.28.1 (_2019-05-21_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.28.0...v0.28.1)

This release exists only to rectify a publishing error that occurred with v0.28.0.

# v0.28.0 (_2019-05-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.27.2...v0.28.0)

## FxA

### Breaking Changes

- `FirefoxAccount.ensureCapabilities` now takes a set of capabilities
   as a parameter. All the device registered "capabilities" such as Send
   Tab will be replaced by the passed set of new capabilities.

## Push

### Breaking Changes

- `PushManager.verifyConnection()` now returns a boolean. `true`
  indicates the connection is valid and no action required, `false`
indicates that the connection is invalid. All existing subscriptions
have been dropped. The caller should send a `pushsubscriptionchange`
to all known apps. (This is due to the fact that the Push API does
not have a way to send just the new endpoint to the client PWA.)
[#1114](https://github.com/mozilla/application-services/issues/1114)

- `PushManager.unsubscribe(...)` now will only unsubscribe a single
  channel. It will return `false` if no channel is specified or if the
channel was already deleted. To delete all channels for a given user,
call `PushManager.unsubscribeAll()`.
[#889](https://github.com/mozilla/application-services/issues/889)

## General

### What's Fixed

- Native libraries should now have debug symbols stripped by default,
  resulting in significantly smaller package size for consuming
  applications. A test was also added to CI to ensure that this
  does not regress in future.
  ([1107](https://github.com/mozilla/application-services/issues/1107))


# v0.27.2 (_2019-05-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.27.1...v0.27.2)

## Logins

### What's new

- iOS only: Logins store has a new (static) `numOpenConnections` function, which can be used to detect leaks. ([#1070](https://github.com/mozilla/application-services/pull/1070))

## Places

### What's New

- iOS only: PlacesApi can now migrate bookmark data from a `browser.db` database
  via the `migrateBookmarksFromBrowserDb` function. It is recommended that this
  only be called for non-sync users, as syncing the bookmarks over will result
  in better handling of sync metadata, among other things.
  ([#1078](https://github.com/mozilla/application-services/pull/1078))
- iOS: Sync can now be interrupted using the `interrupt` method
  ([#1092](https://github.com/mozilla/application-services/pull/1092))
- iOS: Sync metadata can be reset using the `resetBookmarksMetadata` method
  ([#1092](https://github.com/mozilla/application-services/pull/1092))


# v0.27.1 (_2019-04-26_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.27.0...v0.27.1)

## FxA

### What's New

- Added `destroyDevice` support to existing Send Tab capabilities. ([#821](https://github.com/mozilla/application-services/pull/821))

## Places

### What's New

- Frecencies are now recalculated for bookmarked URLs after a sync.
  ([#847](https://github.com/mozilla/application-services/issues/847))

## Push

### What's Fixed

- Authentication failures with the autopush server should be fixed. ([#1080](https://github.com/mozilla/application-services/pull/1080))

# v0.27.0 (_2019-04-22_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.26.2...v0.27.0)

## General

- JNA has been updated to version 5.2.0 (previously 4.5.2) ([#1057](https://github.com/mozilla/application-services/pull/1057))

- SQLCipher has been updated to version 4.1.0 (previously 4.0.0) ([#1060](https://github.com/mozilla/application-services/pull/1060))

- `android-components` has been updated to 0.50.0 (previously 0.49.0) ([#1062](https://github.com/mozilla/application-services/pull/1062))

- SQLCipher should no longer be required in megazords which do not contain `logins`. ([#996](https://github.com/mozilla/application-services/pull/996))

- Non-megazord builds should once again work ([#1046](https://github.com/mozilla/application-services/pull/1046))

## FxA

### What's New

- New methods `getManageAccountURL` and `getManageDevicesURL` have been added,
  which the application can use to direct the user to manage their account on the web.
  ([#984](https://github.com/mozilla/application-services/pull/984))
- Android only: Added device registration and Firefox Send Tab capability support. Your app can opt into this by calling the `FirefoxAccount.initializeDevice` method. ([#676](https://github.com/mozilla/application-services/pull/676))

- Switched to use the new fxa-auth-server token endpoint which generates device records, email and push notifications
 for connected clients([#1055](https://github.com/mozilla/application-services/pull/1055))

## Places

### Breaking Changes

- It is no longer possible to create an encrypted places database. ([#950](https://github.com/mozilla/application-services/issues/950))
- `syncBookmarks()` API is now marked `open` to be accessible outside the framework. ([#1058](https://github.com/mozilla/application-services/issues/1058))

### What's Fixed

- Non-megazord builds should once again function. ([#1045](https://github.com/mozilla/application-services/issues/1045))

# v0.26.2 (_2019-04-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.26.1...v0.26.2)

## iOS Framework

### What's Fixed

- iOS temporarially no longer uses NSS for crypto. This is a short term fix to
  allow firefox-ios to release an update.

# v0.26.1 (_2019-04-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.26.0...v0.26.1)

## iOS Framework

### What's Fixed

- iOS networking should use the reqwest backend, instead of failing ([#1032](https://github.com/mozilla/application-services/pull/1032))

# v0.26.0 (_2019-04-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.25.2...v0.26.0)

## Gradle plugin

- Removed the appservices bintray repo from the plugin ([#899](https://github.com/mozilla/application-services/issues/899))

## Push

### Breaking Change

- `PushAPI.subscribe()` now returns a `SubscriptionResponse` that contains the server supplied `channelID` and the
   `subscriptionInfo` block previously returned. Please note: the server supplied `channelID` may differ from the
   supplied `channelID` argument. This is definitely true when an empty channelID value is provided to `subscribe()`,
   or if the channelID is not a proper UUID.
   The returned `channelID` value is authoritative and will be the value associated with the subscription and future
   subscription updates. As before, the `subscriptionResponse.subscriptionInfo` can be JSON serialized and returned to the application.
   ([#988](https://github.com/mozilla/application-services/pull/988))

## Places

### What's new

- Bookmarks may now be synced using the `syncBookmarks` method on `PlacesApi`
  (and on Android, the interface it implements, `SyncManager`).
  ([#850](https://github.com/mozilla/application-services/issues/850))
- Android only: New methods for querying paginated history have been added:
  `getVisitCount` and `getVisitPage`
  ([#992](https://github.com/mozilla/application-services/issues/992))
- Android only: `getVisitInfos` now takes a list of visit types to exclude.
  ([#920](https://github.com/mozilla/application-services/issues/920))

### Breaking Changes

- Android only: The addition of `syncBookmarks` on the `PlacesManager` interface
  is a breaking change. ([#850](https://github.com/mozilla/application-services/issues/850))
- Android only: `sync` has been renamed to `syncHistory` for clarity given the
  existence of `syncBookmarks`.
  ([#850](https://github.com/mozilla/application-services/issues/850))
- Android only: `getVisitInfos` has changed, which is breaking for implementors
  of `ReadableHistoryConnection`.
  ([#920](https://github.com/mozilla/application-services/issues/920))
- Android only: New methods on `ReadableHistoryConnection`: `getVisitCount` and
  `getVisitPage`.
  ([#992](https://github.com/mozilla/application-services/issues/992))

## Logins

### What's new

- iOS only: Logins operations may now be interrupted via the `interrupt()`
  method on LoginsDb, which may be called from any thread.
  ([#884](https://github.com/mozilla/application-services/issues/884))
    - This is currently only implemented for iOS due to lack of interest on the
      Android side, please let us know if this is desirable in the Android API
      as well. Feel free to indicate support for exposing this in the Android API
      [here](https://github.com/mozilla/application-services/issues/1020).

# v0.25.2 (_2019-04-11_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.24.0...v0.25.2)

## General

- Some cryptographic primitives are now backed by NSS. On reference-browser and fenix megazords the GeckoView NSS libs are used, otherwise these libraries are bundled. ([#891](https://github.com/mozilla/application-services/pull/891))

### What's Fixed

- Megazords and requests should work again. ([#946](https://github.com/mozilla/application-services/pull/946))
- The vestigial `reqwest` backend is no longer compiled into the megazords ([#937](https://github.com/mozilla/application-services/pull/937)).
    - Note that prior to this it was present, but unused.

## iOS

- The individual components projects have been removed, please use the MozillaAppServices framework from now on. ([#932](https://github.com/mozilla/application-services/pull/932))
- The NSS .dylibs must be included in your application project, see [instructions](https://github.com/mozilla/application-services/blob/30a1a57917c6e243c0c5d59fba24caa8de8f6b3a/docs/howtos/consuming-rust-components-on-ios.md#nss)

## Push

### What's fixed

- PushAPI now stores some metadata information across restarts ([#905](https://github.com/mozilla/application-services/issues/905))

# v0.24.0 (_2019-04-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.23.0...v0.24.0)

## Megazords

## Breaking Changes

- Megazord initialization has changed. Megazords' init() function now takes a
  `Lazy<mozilla.components.concept.fetch.Client>` (from
  [concept-fetch](https://github.com/mozilla-mobile/android-components/tree/master/components/concept/fetch/)),
  which will be used to proxy all HTTP requests through. It will not be accessed
  until a method is called on rust code which requires the network. This
  functionality is not present in non-megazords. ([#835](https://github.com/mozilla/application-services/pull/835))

    An example of how to initialize this follows:

    ```kotlin
    val megazordClass = Class.forName("mozilla.appservices.MyCoolMegazord")
    val megazordInitMethod = megazordClass.getDeclaredMethod("init", Lazy::class.java)
    val lazyClient: Lazy<Client> = lazy { components.core.client }
    megazordInitMethod.invoke(megazordClass, lazyClient)
    ```

    Or (if you don't have GeckoView available, e.g. in the case of lockbox):

    ```kotlin
    val megazordClass = Class.forName("mozilla.appservices.MyCoolMegazord")
    val megazordInitMethod = megazordClass.getDeclaredMethod("init", Lazy::class.java)
    // HttpURLConnectionClient is from mozilla.components.lib.fetch.httpurlconnection
    val lazyClient: Lazy<Client> = lazy { HttpURLConnectionClient() }
    megazordInitMethod.invoke(megazordClass, lazyClient)
    ```

## General

- Native code builds are now stripped by default, reducing size by almost an
  order of magnitude. ([#913](https://github.com/mozilla/application-services/issues/913))
    - This is done rather than relying on consumers to strip them, which proved
      more difficult than anticipated.

## Push

### What's new

- PushAPI now defines a number of default parameters for functions ([#868](https://github.com/mozilla/application-services/issues/868))

### Breaking changes

- `mozilla.appservices.push.BridgeTypes` is now
  `mozilla.appservices.push.BridgeType`
([#885](https://github.com/mozilla/application-services/issues/885))

## Places

### What's Fixed

- Swift PlacesAPI methods are not externally accessible
  ([#928](https://github.com/mozilla/application-services/issues/928))

# v0.23.0 (_2019-03-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.22.1...v0.23.0)

## Places

### What's Fixed

- createBookmarkItem on android will now create the correct type of bookmark.
  ([#880](https://github.com/mozilla/application-services/issues/880))

## Push

### Breaking changes

- the `PushManager` argument `socket_protocol` is now `http_protocol`
  to correctly map its role. `socket_protocol` is reserved.

# v0.22.1 (_2019-03-27_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.22.0...v0.22.1)

## Logins

### What's New

- iOS Logins storage now has `ensureLocked`, `ensureUnlocked`, and `wipeLocal`
  methods, equivalent to those provided in the android API.
  ([#854](https://github.com/mozilla/application-services/issues/854))

## Places

### What's Fixed

- PlacesAPIs should now be closed when all references to them are no longer used.
  ([#749](https://github.com/mozilla/application-services/issues/749))

# v0.22.0 (_2019-03-22_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.21.0...v0.22.0)

## Logins

- Added a disableMemSecurity function to turn off some dubious behaviors of SQLcipher. ([#838](https://github.com/mozilla/application-services/pull/838))
- The iOS SQLCipher build configuration has been adjusted ([#837](https://github.com/mozilla/application-services/pull/837))

## Push

### Breaking changes

- `PushManager`'s `dispatch_for_chid` method has been renamed to `dispatchForChid`.
- `PushManager` constructor arguments are now camelCased.

## `org.mozilla.appservices` Gradle plugin

- Artifacts are now to be published to the `mozilla-appservices` bintray organization.  This necessitates version 0.4.3 of the Gradle plugin.  ([#843](https://github.com/mozilla/application-services/issues/843))

# v0.21.0 (_2019-03-20_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.20.2...v0.21.0)

## General

- Breakpad symbols should be available for android now ([#741](https://github.com/mozilla/application-services/pull/741))

## Places

- Places now is available on iOS, however support is limited to Bookmarks. ([#743](https://github.com/mozilla/application-services/pull/743))
- Places now has bookmarks support enabled in the FFI. This addition is too large to include in the changelog, however both Swift and Kotlin APIs for this are fairly well documented. ([#743](https://github.com/mozilla/application-services/pull/743))


# v0.20.2 (_2019-03-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.20.1...v0.20.2)

- An automation problem with the previous release, forcing a version bump. No functional changes.
- Local development: non-megazord builds are now `debug` be default, improving local build times
and working around subtle build issues.
- Override this via a flag in `local.properties`: `application-services.nonmegazord-profile=release`

# v0.20.1 (_2019-03-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.20.0...v0.20.1)

- A error in the build.gradle file caused the v0.20.0 release to fail, this
  release should not be meaningfully different from it.

# v0.20.0 (_2019-03-14_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.19.0...v0.20.0)

## General

- The previous release had an issue with the megazords, and so another
  release was needed. This is version 0.4.2 of the megazord plugin.
  ([#775](https://github.com/mozilla/application-services/pull/775))

### Breaking Changes

- All package names have been normalized. The gradle packages should all be
  `org.mozilla.appservices:component`, and the java namespaces should be
  `mozilla.appservices.component`. ([#776](https://github.com/mozilla/application-services/pull/776))

## Logins

### Breaking Changes

- The gradle package for logins has been changed from
  `'org.mozilla.sync15:logins'` to `org.mozilla.appservices:logins`.
  ([#776](https://github.com/mozilla/application-services/pull/776))

## Places

### Breaking Changes

- Several classes and interfaces have been renamed after feedback from consumers
  to avoid `Interface` in the name, and better reflect what they provide.
    - `PlacesApiInterface` => `PlacesManager`
    - `PlacesConnectionInterface` => `InterruptibleConnection`
    - `ReadablePlacesConnectionInterface` => `ReadableHistoryConnection`
    - `WritablePlacesConnectionInterface` => `WritableHistoryConnection`
    - `ReadablePlacesConnection` => `PlacesReaderConnection`
    - `WritablePlacesConnection` => `PlacesWriterConnection`

- The java namespace used in places has changed from `org.mozilla.places` to
  `mozilla.appservices.places`
  ([#776](https://github.com/mozilla/application-services/pull/776))

- The gradle package for places has been changed from
  `'org.mozilla.places:places'` to `org.mozilla.appservices:places`.
  ([#776](https://github.com/mozilla/application-services/pull/776))

## FxA

### Breaking Changes

- The gradle package for fxa-client has been changed from
  `'org.mozilla.fxaclient:fxaclient'` to `org.mozilla.appservices:fxaclient`.
  ([#776](https://github.com/mozilla/application-services/pull/776))

# 0.19.0 (_2019-03-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.18.0...v0.19.0)

## General

### What's New

- Initial support for the new Push component landed, however it's not yet ready
  for widespread use ([#683](https://github.com/mozilla/application-services/pull/683))

## Places

### What's New

- A massive rewrite of the Kotlin API has been completed. This distinguishes
  reader and writer connections. A brief description of the new types follows.
  Note that all the types have corresponding interfaces that allow for them to
  be mocked during testing as needed. ([#718](https://github.com/mozilla/application-services/pull/718))
    - `PlacesApi`: This is similar to a connection pool, it exists to give out
      reader and writer connections via the functions `openReader` and
      `getWriter`. The naming distinction is due to there only being a single
      writer connection (which is actually opened when the `PlacesApi` is
      created). This class generally should be a singleton.
        - In addition to `openReader` and `getWriter`, this also includes the
        `sync()` method, as that requires a special type of connection.
    - `ReadablePlacesConnection`: This is a read-only connection to the places
      database, implements all the methods of the API that do not require write
      access.
        - Specifically, `getVisited`, `matchUrl`, `queryAutocomplete`, `interrupt`,
          `getVisitedUrlsInRange`, and `getVisitInfos` all exist on this object.
    - `WritablePlacesConnection`: This is a read-write connection, and as such,
      contains not only the all reader methods mentioned above, but also the
      methods requiring write access, such as:
        - `noteObservation`, `wipeLocal`, `runMaintenance`, `pruneDestructively`,
          `deleteEverything`, `deletePlace`, `deleteVisitsSince`, `deleteVisitsBetween`,
          and `deleteVisit`.
    - Note that the semantics of the various methods have not been changed, only
      their location.

### Breaking Changes

- Almost the entire API has been rewritten. See "What's New" for
  details. ([#718](https://github.com/mozilla/application-services/pull/718))

# 0.18.0 (_2019-02-27_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.17.0...v0.18.0)

## FxA

### Breaking Changes

- Swift: `FxAError` has been renamed to `FirefoxAccountError` ([#713](https://github.com/mozilla/application-services/pull/713))

## Places

### What's Fixed

- Autocomplete should no longer return an error when encountering certain emoji ([#691](https://github.com/mozilla/application-services/pull/691))

## Logging

### What's New

- The `rc_log` component now has support for iOS. It is only available as part of the
  MozillaAppServices megazord. ([#618](https://github.com/mozilla/application-services/issues/618))

# 0.17.0 (_2019-02-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.16.1...v0.17.0)

## FxA

### What's New

- We are now using [Protocol Buffers](https://developers.google.com/protocol-buffers/) to pass the Profile data across the FFI boundaries, both on Android and iOS. On Android there should be no breaking changes.
- Kotlin: `Profile` is now a [Data Class](https://kotlinlang.org/docs/reference/data-classes.html).

### Breaking changes

- iOS: You now have to include the `SwiftProtobuf` framework in your projects for FxAClient to work (otherwise you'll get a runtime error when fetching the user profile). It is built into `Carthage/Build/iOS` just like `FxAClient.framework`.
- iOS: In order to build FxAClient from source, you need [swift-protobuf](https://github.com/apple/swift-protobuf) installed. Simply run `brew install swift-protobuf` if you have Homebrew.
- iOS: You need to run `carthage bootstrap` at the root of the repository at least once before building the FxAClient project: this will build the `SwiftProtobuf.framework` file needed by the project.
- iOS: the `Profile` class now inherits from `RustProtobuf`. Nothing should change in practice for you.

## Places

### What's New

- New methods on PlacesConnection (Breaking changes for classes implementing PlacesAPI):
    - `fun deleteVisit(url: String, timestamp: Long)`: If a visit exists at the specified timestamp for the specified URL, delete it. This change will be synced if it is the last remaining visit (standard caveat for partial visit deletion). ([#621](https://github.com/mozilla/application-services/issues/621))
    - `fun deleteVisitsBetween(start: Long, end: Long)`: Similar to `deleteVisitsSince(start)`, but takes an end date. ([#621](https://github.com/mozilla/application-services/issues/621))
    - `fun getVisitInfos(start: Long, end: Long = Long.MAX_VALUE): List<VisitInfo>`: Returns a more detailed set of information about the visits that occured. ([#619](https://github.com/mozilla/application-services/issues/619))
        - `VisitInfo` is a new data class that contains a visit's url, title, timestamp, and type.
    - `fun wipeLocal()`: Deletes all history entries without recording any sync information. ([#611](https://github.com/mozilla/application-services/issues/611)).

        This means that these visits are likely to start slowly trickling back
        in over time, and many of them will come back entirely if a full sync
        is performed (which may not happen for some time, admittedly). The
        intention here is that this is a method that's used if data should be
        discarded when disconnecting sync, assuming that it would be desirable
        for the data to show up again if sync is reconnected.

        For more permanent local deletions, see `deleteEverything`, also added
        in this version.

    - `fun runMaintenance()`: Perform automatic maintenance. ([#611](https://github.com/mozilla/application-services/issues/611))

        This should be called at least once per day, however that is a
        recommendation and not a requirement, and nothing dire happens if it is
        not called.

        The maintenance it may perform potentially includes, but is not limited to:

        - Running `VACUUM`.
        - Requesting that SQLite optimize our indices.
        - Expiring old visits.
        - Deleting or fixing corrupt or invalid rows.
        - Etc.

        However not all of these are currently implemented.

    - `fun pruneDestructively()`: Aggressively prune history visits. ([#611](https://github.com/mozilla/application-services/issues/611))

        These deletions are not intended to be synced, however due to the way
        history sync works, this can still cause data loss.

        As a result, this should only be called if a low disk space notification
        is received from the OS, and things like the network cache have already
        been cleared.

    - `fun deleteEverything()`: Delete all history visits. ([#647](https://github.com/mozilla/application-services/issues/647))

        For sync users, this will not cause the visits to disappear from the
        users remote devices, however it will prevent them from ever showing
        up again, even across full syncs, or sync sign-in and sign-out.

        See also `wipeLocal`, also added in this version, which is less
        permanent with respect to sync data (a full sync is likely to bring
        most of it back).


### Breaking Changes

- The new `PlacesConnection` methods listed in the "What's New" all need to be implemented (or stubbed) by any class that implements `PlacesAPI`. (multiple bugs, see "What's New" for specifics).

### What's fixed

- Locally deleted visits deleted using `deleteVisitsSince` should not be resurrected on future syncs. ([#621](https://github.com/mozilla/application-services/issues/621))
- Places now properly updates frecency for origins, and generally supports
  origins in a way more in line with how they're implemented on desktop. ([#429](https://github.com/mozilla/application-services/pull/429))

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
