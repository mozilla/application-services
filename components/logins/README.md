# Logins Component

![status-img](https://img.shields.io/static/v1?label=production&message=Lockwise,%20Firefox%20iOS&color=darkgreen)
![status-img](https://img.shields.io/static/v1?label=not%20implemented&message=Firefox%20Preview,%20Desktop&color=darkred)

## Implementation Overview
Logins implements encrypted storage for login records on top of SQLcipher, with support for Sync (using the [sync15](https://github.com/mozilla/application-services/tree/master/components/sync15) crate). It used a modified version of the database schema that [firefox-ios](https://github.com/mozilla-mobile/firefox-ios/blob/faa6a2839abf4da2c54ff1b3291174b50b31ab2c/Storage/SQL/SQLiteLogins.swift) used.  Notable difference include:
- the queries
- how sync is performed in order to allow syncs to complete with fewer database operations
- timestamps, iOS uses microseconds, where the logins component uses milliseconds.

See the header comment in `src/schema.rs` for an overview of the schema.

## Directory structure
The relevant directories are as follows:

- `src`: The meat of the library. This contains cross-platform rust code that
  implements the actual storage and sync of login records.
- `example`: This contains example rust code for syncing, displaying, and
  editing logins using the code in `src`.
- `ffi`: The Rust public FFI bindings. This is a (memory-unsafe, by necessity)
  API that is exposed to Kotlin and Swift. It leverages the `ffi_support` crate
  to avoid many issues and make it more safe than it otherwise would be. At the
  time of this writing, it uses JSON for marshalling data over the FFI, however
  in the future we will likely use protocol buffers.
- `android`: This contains android bindings to logins, written in Kotlin. These
  use JNA to call into to the code in `ffi`.
- `ios`: This contains the iOS binding to logins, written in Swift. These use
  Swift's native support for calling code written in C to call into the code in
  `ffi`.

## Features
1. Locally encrypted storage of username and password information
1. Create, Update and Delete (CRUD) operations for login data
1. Syncing of logins for devices and apps connected via Firefox Accounts
1. Import functionality from existing login storage (ex: Fx Desktop or Fennec)
1. Data migration functionality from Fennec to Firefox Preview storage


## Business Logic

### Record storage

At any given time records can exist in 3 places, the local storage, the remote record, and the shared parent.  The shared parent refers to a record that has been synced previously and is referred to in the code as the mirror. Login records are encrypted and stored locally. For any record that does not have a shared parent the login component tracks that the record has never been synced.

Reference the [Logins chapter of the synconomicon](https://mozilla.github.io/application-services/synconomicon/ch01.1-logins.html) for detailed information on the record storage format.

### Sign-out behavior
When the user signs out of their Firefox Account, we reset the storage and clear the shared parent.

### Merging records
When records are added, the logins component performs a three-way merge between the local record, the remote record and the shared parent (last update on the server).  Details on the merging algorithm are contained in the [generic sync rfc](https://github.com/mozilla/application-services/blob/1e2ba102ee1709f51d200a2dd5e96155581a81b2/docs/design/remerge/rfc.md#three-way-merge-algorithm).

### Record de-duplication

De-duplication compares the records for same the username and same url, but with different passwords. De-duplication compares the records for same the username and same url, but with different passwords.  Deduplication logic is based on age, the username and hostname.
- If the changes are more recent than the local record it performs an update.
- If the change is older than our local records, and you have changed the same field on both, the record is not updated.

## Getting started

**Prerequisites**: Firefox account authentication is necessary to obtain the keys to decrypt synced login data.  See the [android-components FxA Client readme](https://github.com/mozilla-mobile/android-components/blob/master/components/service/firefox-accounts/README.md) for details on how to implement on Android.  For iOS, Firefox for iOS still implement the legacy oauth.

**Platform-specific details**:
- Android: add support for the logins component via android-components [Firefox Sync - Logins](https://github.com/mozilla-mobile/android-components/blob/master/components/service/sync-logins/README.md) service.
- iOS: start with the [guide to consuming rust components on iOS](https://github.com/mozilla/application-services/blob/master/docs/howtos/consuming-rust-components-on-ios.md)

## API Documentation
- TODO [Expand and update API docs](https://github.com/mozilla/application-services/issues/1747)

## Testing

![status-img](https://img.shields.io/static/v1?label=test%20status&message=acceptable&color=darkgreen)

Our goal is to seek an _acceptable_ level of test coverage. When making changes in an area, make an effort to improve (or minimally not reduce) coverage. Test coverage assessment includes:
* [rust tests](https://github.com/mozilla/application-services/blob/master/testing/sync-test/src/logins.rs)
* [android tests](https://github.com/mozilla/application-services/tree/master/components/logins/android/src/test/java/mozilla/appservices/logins)
* [ios tests](https://github.com/mozilla/application-services/blob/master/megazords/ios/MozillaAppServicesTests/LoginsTests.swift)
* TODO [measure and report test coverage of logins component](https://github.com/mozilla/application-services/issues/1745)

## Telemetry
- TODO [implement logins sync ping telemety via glean](https://github.com/mozilla/application-services/issues/1867)
- TODO [Define instrument and measure success metrics](https://github.com/mozilla/application-services/issues/1749)
- TODO [Define instrument and measure quality metrics](https://github.com/mozilla/application-services/issues/1748)

## Examples
- [Android integration](https://github.com/mozilla-mobile/android-components/blob/master/components/service/sync-logins/README.md)
