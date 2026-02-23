# v150.0 (In progress)

[Full Changelog](In progress)

# v149.0 (_2026-02-23_)

## ⚠️ Breaking Changes ⚠️

### General
* Updated UniFFI to 0.31 ([#7140](https://github.com/mozilla/application-services/issues/7140))

### Glean
* Updated to v67.0.0 ([#7177](https://github.com/mozilla/application-services/issues/7177))

### Nimbus
* Added `eval_jexl_debug()` method to `NimbusTargetingHelper` interface for CLI testing and debugging. Evaluates JEXL expressions and returns debug results as JSON. Consumers implementing this interface must add the new method.
([#7156](https://github.com/mozilla/application-services/pull/7156))
([#31607](https://github.com/mozilla-mobile/firefox-ios/pull/31607))
* Update Cirrus `MetricsHandler` interface for recording enrollment status to specify nimbus user id as separate metric and change method name from `record_enrollment_statuses` to `record_enrollment_statuses_v2`. Consumers implementing this interface must add the new method.
([#14280](https://github.com/mozilla/experimenter/pull/14280))
* Move `nimbus_events.enrollment_status` to new `nimbus-targeting-context` ping, and add Nimbus `MetricsHandler` interface method `submit_targeting_context` to submit the ping. Consumers implementing this interface must add the new method. ([#14542](https://github.com/mozilla/experimenter/issues/14542))
* Enable using `PreviousGeckoPrefState` to revert Gecko pref experiments when applicable ([#7157](https://github.com/mozilla/application-services/pull/7157))

### Error support
* Removed the `tracing-logging` and `tracing-reporting` features, these are now always enabled.
  We don't believe this should affect any consumers, since they're were already using the
  `tracing-logging` feature and were either also using `tracing-reporting` or not handling error
  reporting at all.
* Reworked `register_event_sink` signature to allow it to register an event sink for muliple targets at once.
* Reworked `unregister_event_sink`.  It now inputs the return value from `register_event_sink`.
* Removed `register_min_level_event_sink` and `unregister_min_level_event_sink`.
  Use the new `register_event_sink` instead.

### Logins
* Opened count method on logins for Android. ([#7207](https://github.com/mozilla/application-services/pull/7207/))

### Autofill
* Added count methods for credit cards and addresses. ([#7207](https://github.com/mozilla/application-services/pull/7207/))

## ✨ What's New ✨

### Ads Client
* Adds new Kotlin `AdsClientTelemetry.kt` wrapper for Glean callbacks.
* Try to reset cache database schema on connection initialization failure.
* Reset cache on context ID rotation.
* Enable staging environment support for all platforms (previously feature-gated)
* Temporarily disable cache invalidation on click and impression recording (will be re-enabled behind Nimbus experiment)
* Enable automatic context_id rotation every 3 days
* **BREAKING**: Removed `cycle_context_id()` API method - context_id rotation is now automatic
* Modified HTTP cache to ignore `context_id` field in request bodies when generating cache keys, preventing unnecessary cache invalidation on rotation

### Android
* Upgraded Kotlin compiler from 2.2.21 to 2.3.0 ([#7183](https://github.com/mozilla/application-services/pull/7183))

### FxA Client
* Support for the token exchange API, which we plan to use for getting access tokens for Relay.
  ([#7179](https://github.com/mozilla/application-services/pull/7179)).
* Removed `SwiftKeychainWrapper` logic. ([#7150](https://github.com/mozilla/application-services/pull/7150))

### Logins
- Added `runMaintenance` API to `DatabaseLoginsStorage`

### Nimbus
* Adds a `Vec<PreviousGeckoPrefState>` on `ExperimentEnrollment` when it is of type `EnrollmentStatus::Enrolled` and getters and setters. This is to support returning to an original value on Gecko pref experiments.
* Added `eval-jexl` command to nimbus-cli for evaluating JEXL targeting expressions against the app context. Useful for testing and debugging targeting expressions on iOS and Android.
([#7160](https://github.com/mozilla/application-services/pull/7160))
* Added Android support for eval-jexl functionality through the `NimbusTargetingHelper.evalJexl()` method, enabling JEXL expression evaluation on Android with full targeting context support.
([#7163](https://github.com/mozilla/application-services/pull/7163))
* Fixed nimbus-cli eval-jexl command to work reliably on Android by removing logcat filters, clearing logs before evaluation, and increasing retry timing for better device compatibility.
([#7173](https://github.com/mozilla/application-services/pull/7173))
* Added `recordEventOrThrow()` method to Nimbus Android SDK, allowing callers to catch database errors when recording events. Unlike `recordEvent()`, this method does not suppress exceptions, enabling error handling in consumers like Fenix.

### Logins
- Added `runMaintenance` API to `DatabaseLoginsStorage`
- Add password reuse detection for breach alerts: Database schema upgraded to version 4 with new `breachesL` table storing encrypted breached passwords. New APIs `are_potentially_vulnerable_passwords()` (batch check) and `is_potentially_vulnerable_password()` (single check) enable cross-domain password reuse detection.
- Add `record_potentially_vulnerable_passwords()` API for bulk-inserting breached passwords into the breach database. This is used during import operations (`add_many_with_meta()`) to automatically populate the breach database with passwords from logins with known breaches.
- Move breach alert fields (`time_of_last_breach`, `time_last_breach_alert_dismissed`) from `LoginFields` to `LoginMeta` to group internally managed fields that are not directly updateable via the `update()` API.

### Ads-Client
* Adds new Kotlin `AdsClientTelemetry.kt` wrapper for Glean callbacks.

### Relay

* Added `X-Relay-Client` header to all Relay API requests with automatic platform detection (`appservices-ios`, `appservices-android`, etc.) to help the backend distinguish mobile vs desktop requests for telemetry.

### Viaduct
* Support setting default user-agent headers.

### Error support
* Added the `RustComponentsErrorTelemetry.submitErrorPing` method to allow Android consumers to submit rust components error pings.

## 🔧 What's Fixed 🔧

### Remote Settings
* Removed potential deadlock (https://bugzilla.mozilla.org/show_bug.cgi?id=2012955)

[Full Changelog](https://github.com/mozilla/application-services/compare/v148.0...v149.0)

# v148.0 (_2026-01-12_)

### Logins
- Add breach alert support, including a database migration to version 3,
  new `Login` fields (`time_of_last_breach`, `time_last_breach_alert_dismissed`),
  and new `LoginStore` APIs (`record_breach`, `reset_all_breaches`, `is_potentially_breached`, `record_breach_alert_dismissal_time`, `record_breach_alert_dismissal`, `is_breach_alert_dismissed`). ([#7127](https://github.com/mozilla/application-services/pull/7127))

[Full Changelog](https://github.com/mozilla/application-services/compare/v147.0...v148.0)

## ⚠️ Breaking Changes ⚠️

### Fxa Client
- Removed the optional `ttl` paramater to `get_access_token`.  In practice, no consumers were using this.

## ✨ What's New ✨

### Ads Client
- Add agnostic telemetry support (compatible with Glean)

### Fxa Client
- Added optional `use_cache` paramater to `get_access_token`.  Set this to `false` to force
  requesting a new token.

# v147.0 (_2025-12-07_)

### Relay
- Added `fetch_profile()` method to check premium subscription status via `has_premium` field ([#7113](https://github.com/mozilla/application-services/pull/7113))

### Nimbus

### ⚠️ Breaking Changes ⚠️
- Removed unused `home_directory` field from AppContext. Both Kotlin and Swift sides were passing null values and it wasn't used anywhere. ([#7085](https://github.com/mozilla/application-services/pull/7085)) ([#30782](https://github.com/mozilla-mobile/firefox-ios/pull/30782))

### `rc_crypto`
- Thread-safety improvements for PKCS-token-dependent methods by introducing a
  global mutex. Refactored key unpacking logic and removed redundant code;
  includes some breaking API changes, but since the keydb feature is not yet in
  use, these do not affect existing consumers.
  - `get_aes256_key` now returns a `Result<Option<Key>>` to distinguish missing
    keys from errors
  - `get_or_create_aes256_key` only creates a key when none exists.
  - When the keydb feature is enabled, `ensure_nss_initialized` is disabled in
    favor of `ensure_nss_initialized_with_profile_dir`.

### Logins
- `create_login_store_with_nss_keymanager` returns an `ApiResult` now, instead
  of just panicking.
- fix `count_by_origin` and `count_by_form_action_origin` with punicode origins

### Places
- `places::storage::history_metadata::get_most_recent_search_entries()` was added to fetch the most recent search entries in history metadata. ([#7104](https://github.com/mozilla/application-services/pull/7104))
- `places::storage::history_metadata::delete_all_metadata_for_search()` was added to delete the search terms in history metadata. ([#7101](https://github.com/mozilla/application-services/pull/7101))

[Full Changelog](https://github.com/mozilla/application-services/compare/v146.0...v147.0)

# v146.0 (_2025-11-10_)

## ✨ What's New ✨

### Ads Client
- Add support for three ad types: Image, Spoc (Sponsored Content), and UA Tile

### Autofill
- Adds a migration to migrate users to use subregion codes over fully qualified strings. ([bug 1993388](https://bugzilla.mozilla.org/show_bug.cgi?id=1993388))
- Added credit card verification logic ([#7047](https://github.com/mozilla/application-services/pull/7047)).

### Relay
- Added Remote Settings integration to determine site eligibility for displaying Relay UI. The new `RelayRemoteSettingsClient` fetches allowlist/denylist data, and `should_show_relay()` provides subdomain-aware domain matching to decide when to show Relay email mask suggestions. ([#7039](https://github.com/mozilla/application-services/pull/7039))

## 🦊 What's Changed 🦊

### Android
- Upgraded NDK from r28c to r29. ([#7014](https://github.com/mozilla/application-services/pull/7014))

### Glean
- Updated to v66.0.0 ([#7025](https://github.com/mozilla/application-services/issues/7025))

### Nimbus
- The `participation` field is no longer required in the Cirrus
  `EnrollmentRequest` type. Instead, when users opt-out, the client application
  should no longer send enrollment requests to Cirrus.
  ([#7030](https://github.com/mozilla/application-services/pull/7030))

[Full Changelog](https://github.com/mozilla/application-services/compare/v145.0...v146.0)

# v145.0 (_2025-10-13_)

## ✨ What's New ✨

### Swift
- Added `@unchecked Sendable` to classes that conform to `FeatureManifestInterface`. ([#6963](https://github.com/mozilla/application-services/pull/6963))

### Ads Client
- Added the Ads Client component to the Megazord.
- Updated the ApiError enum to AdsClientApiError to avoid naming collision.
- The `context_id` is now generated and rotated via the existing eponym component crate.
- Added request caching mechanism using SQLite with configurable TTL and max size.
- Added configuration options for the cache.
- Deserialize callbacks with `url::Url`
- Support for multiple ads request (with count)

### Relay
- **⚠️ Breaking Change:** The error handling for the Relay component has been refactored for stronger forward compatibility and more transparent error reporting in Swift and Kotlin via UniFFI.
    - API and network errors from the Relay server are now converted to a single `RelayApiError::Api { status, code, detail }` variant, exposing the HTTP status code, a machine-readable error code (if present), and a human-readable detail message.
    - Downstream client apps can now handle server errors based on both the `status` and `error_code` fields directly, without additional changes to the Rust component - even as server-side error codes evolve.
    - **Consumers must update their error handling code to match the new `Api { status, code, detail }` shape.**

### Places
- `places::storage::history_metadata::get_most_recent(limit: i32)` was added to get most recent history metadata limited to a number. ([#7002](https://github.com/mozilla/application-services/pull/7002))

### FxA Client
- Expose `getAttachedClients` from the uniffi layer in the Android wrapper.

## 🦊 What's Changed 🦊

### Docs
- Updated the components strategy doc to better reflect the current state of application services. ([#6991](https://github.com/mozilla/application-services/pull/6991))

[Full Changelog](https://github.com/mozilla/application-services/compare/v144.0...v145.0)

# v144.0 (_2025-09-15_)

## ✨ What's New ✨

### OHTTP Client
- The `as-ohttp-client` component is being reintroduced to allow firefox-ios to
  optionally submit Glean pings over OHTTP.

## 🦊 What's Changed 🦊

### Suggest
- Switched from `unicode-normalization` and `unicase` to ICU4X. (And updated the lock file from ICU4X 1.5 to ICU4X 2.0.)

### Android
- Bumped the minimum SDK version to 26 (Android 8). ([#6926](https://github.com/mozilla/application-services/pull/6926)

### Glean
- Updated to v65.0.0 ([#6901](https://github.com/mozilla/application-services/pull/6901))

[Full Changelog](https://github.com/mozilla/application-services/compare/v143.0...v144.0)

## Nimbus CLI
- Support for Firefox for Android and Focus via the new
  [mozilla-firefox](https://github.com/mozilla-firefox/firefox) repository.

# v143.0 (_2025-08-18_)

## 🦊 What's Changed 🦊

### Android
- Upgraded NDK from r28b to r28c. ([#6848](https://github.com/mozilla/application-services/pull/6848))

### Logins
- Updated logins verification telemetry so it can be used in iOS([#6832](https://github.com/mozilla/application-services/pull/6832))
- Updated insert statement to allow updating previously deleted logins via `add_with_meta`.

### Webext-Storage
- Added `get_keys()` method ([bug 1978718](https://bugzilla.mozilla.org/show_bug.cgi?id=1978718))

### Search

- Added `SearchEngineUrl::is_new_until` ([bug 1979962](https://bugzilla.mozilla.org/show_bug.cgi?id=1979962))
- Added `SearchEngineUrl::exclude_partner_code_from_telemetry` ([bug 1980474](https://bugzilla.mozilla.org/show_bug.cgi?id=1980474))
- Added `SearchEngineUrl::accepted_content_types`

### Suggest

- Added namespacing by `suggestion_type` to dismissals of dynamic suggestions ([bug 1983587](https://bugzilla.mozilla.org/show_bug.cgi?id=1983587))

### RC Crypto
- Fix NSS bindings for key management

### Mozilla Ads Client

- Created a new component, `ads-client`

[Full Changelog](https://github.com/mozilla/application-services/compare/v142.0...v143.0)

# v142.0 (_2025-07-21_)

### Logins
- expose constructors for `ManagedEncryptorDecryptor` and `NSSKeyManager`
- change PrimaryPasswordAuthenticator callbacks to be async (a breaking change, but not yet used by anyone)
- return Result in PrimaryPasswordAuthenticator callbacks (again a breaking change, but not yet used by anyone)
- add factory for login store with nss key manager: `create_login_store_with_nss_keymanager` to avoid round-tripping the KeyManager trait interface through JS.
- add LoginStore `shutdown()` function to close database connection
- extend LoginStore `shutdown()` to also remove the encdec

### Nimbus SDK ⛅️🔬🔭
- Updated the Nimbus API Stage URL
- Updated the Nimbus SDK to support setting Gecko preferences ([#6826](https://github.com/mozilla/application-services/pull/6826)).

### Nimbus FML && CLI
- handle http status codes when fetching feature manifests from GitHub.

### Search

- `SearchEngineUrls` now has an optional `visual_search` field, supporting
  visual search endpoints in engine configs.
- `SearchEngineUrl` now has an optional `display_name` field, which is useful if
  a URL corresponds to a brand name distinct from the engine's brand name.

[Full Changelog](https://github.com/mozilla/application-services/compare/v141.0...v142.0)

# v141.0 (_2025-06-23_)

## ✨ What's New ✨

### Filter Adult
- Added the first version of a component for checking if base domains exist in a
  list of base domains for adult websites.

### Relay
- Added the first version of a component for using Firefox Relay.

### Logins
- add checkpoint API: `set_checkpoint(checkpoint)` and `get_checkpoint()` for desktop's rolling migration
- add `delete_many(ids)` for batch deletion within a single transaction
- Add `count()`, `count_by_origin()` and `count_by_form_action_origin()` methods

### Sync Manager
- Added sync settings metrics for mobile. [#6786](https://github.com/mozilla/application-services/pull/6786)

## 🦊 What's Changed 🦊

### Nimbus FML ⛅️🔬🔭🔧
- Updated Nimbus FML to support pref key+branch being set on feature properties (#[6788](https://github.com/mozilla/application-services/pull/6788)).

### Context ID
- The `ContextIDComponent` constructor no longer throws an error. This is not a breaking change since neither iOS or Android implement this yet.
- The `ContextIDComponent` constructor can now synchronously invoke the rotation callback when it receives an invalid timestamp from callers; in such cases, it falls back to the current timestamp and forces an ID rotation.
- `rotate_context_id` is no longer public since consumers can use `force_rotation` instead.

### Glean
- Updated to v64.4.0 ([#6795](https://github.com/mozilla/application-services/pull/6795))

[Full Changelog](https://github.com/mozilla/application-services/compare/v140.0...v141.0)

# v140.0 (_2025-05-23_)

## ⚠️ Breaking Changes ⚠️

### Android
- Added `RustComponentsInitializer.kt` to `init_rust_components`.

### Context ID
- Added the first version of a component for managing and rotating context IDs
  sent to MARS / Merino.

#### BREAKING CHANGE
- Removed `Megazord.kt` and moved the contents to the new `RustComponentsInitializer.kt`.

## 🦊 What's Changed 🦊

### Remote Settings
- `RemoteSettingsService::sync` is now more efficient.  It checks the remote settings changes
  endpoint and only syncs collections that have been modified since the last sync.

### Logins
- add logins store api methods for bulk insert and meta insert, intended to be used during migration and CSV import on desktop:
  - `fn add_with_record(&self, entry_with_record: LoginEntryWithRecordFields)`: add a login together with metadata
  - `fn add_many(&self, entries: Vec<LoginEntry>)`: add multiple logins with single transaction
  - `fn add_many_with_records(&self, entries_with_records: Vec<LoginEntryWithRecordFields>)`: add multiple logins with metadata within single transaction

### Glean
- Updated to v64.3.1 ([#6755](https://github.com/mozilla/application-services/pull/6755)/[#6762](https://github.com/mozilla/application-services/pull/6762))
- Reverted the JNA dependency version back to 5.14.0 due Android 5/6 crashes. ([#6762](https://github.com/mozilla/application-services/pull/6762))

## 🔧 What's Fixed 🔧

### Remote Settings

- Fixed setting a new app context with `RemoteSettingsService::update_config`

### Search

- The `SearchEngineSelector::filter_engine_configuration` will now sort any
  unordered engines by name rather than identifier.
- Added `is_new_until` field to the SearchEngineDefinition struct. This optional field represents the date in YYYY-MM-DD format until which a search engine variant or subvariant is considered "new".

[Full Changelog](https://github.com/mozilla/application-services/compare/v139.0...v140.0)

# v139.0 (_2025-04-28_)

## 🦊 What's Changed 🦊

### Android
- Upgraded the JNA dependency version to 5.17.0. ([#6649](https://github.com/mozilla/application-services/pull/6649))
- Updated to a newer version of Android Components (`139.0.20250417022706`).
- Upgraded NDK from r28 to r28b. ([#6723](https://github.com/mozilla/application-services/pull/6723))

### Glean
- Updated to v64.1.1 ([#6649](https://github.com/mozilla/application-services/pull/6649))/([#6723](https://github.com/mozilla/application-services/pull/6723))

### Logins
- New `NSSKeyManager`, which provides an NSS-backed key manager implementation.
Given a `PrimaryPasswordAuthenticator` implementation, the NSS keystore is used
to store and retrieve the login encryption key. These features are only
available when the Logins component is compiled with the `keydb` feature.
([#6571](https://github.com/mozilla/application-services/pull/6571))

### Sync Pass Example
The `sync-pass` example has been adapted to use the NSSKeyManager. The example
program can be called with an FX profile path in which the key is stored in the
file key4.db and secured with a possibly set primary password.
([#6571](https://github.com/mozilla/application-services/pull/6571))
And it has been polished up a bit: passwords are no longer displayed in plain
text, the list view has been slimmed down and a detail view has been added.
([#6685](https://github.com/mozilla/application-services/pull/6685c))

- Added a function to locally remove corrupted logins. ([#6667](https://github.com/mozilla/application-services/pull/6667))

### Remote Settings
- The `RemoteSettingsService` constructor and `RemoteSettingsService::make_client` no longer perform any IO.
  This integrates better with JS, which expects all IO to happen inside async functions.
- Added `RemoteSettingsClient::close`, which can be used to close the underlying SQLite DB during down.

## ⚠️ Breaking Changes ⚠️

### Remote Settings
- Several methods `RemoteSettingsService` are now infallible, which is a breaking change for Swift code.
    - `RelevancyStore` constructor
    - `RemoteSettingsService` constructor
    - `RemoteSettingsService::make_client()`
    - `SearchEngineSelector::use_remote_settings_server()`
    - `SuggestStore` constructor

### OHTTP Client
- The now-unused `as-ohttp-client` component is now removed. It was previously
  only used by firefox-ios but no longer is.

[Full Changelog](https://github.com/mozilla/application-services/compare/v138.0...v139.0)

# v138.0 (_2025-03-31_)

## ⚠️ Breaking Changes ⚠️

### Remote Settings
- Removed fields from `RemoteSettingsContext`.  The goal is to create a common set of fields between
  the Rust and Desktop clients.  Consumers will need to update their code to stop sending these
  fields.  Also, all fileds now default to `None/null/nil`.
- Made `RemoteSettingsContext::form_factor` and `RemoteSettingsContext::country` top-level fields.

## 🦊 What's Changed 🦊

### Android
- Upgraded Kotlin compiler from 1.9.24 to 2.1.20 ([#6640](https://github.com/mozilla/application-services/pull/6640))/([#6654](https://github.com/mozilla/application-services/pull/6654))

### `nss`
- Initialize nss explicitly ([#6596](https://github.com/mozilla/application-services/pull/6596))

#### BREAKING CHANGE:
Components need to call `nss::ensure_initialized()` before using any component
that depends on NSS. Applications can depend on the `init_rust_components`
component to handle initialization.

### `rc_crypto`
- `ensure_initialized()` now returns a Result

[Full Changelog](https://github.com/mozilla/application-services/compare/v137.0...v138.0)

# v137.0 (_2025-03-03_)

## ✨ What's New ✨

### Merino
- added a client for merino curated recommendations endpoint

## 🦊 What's Changed 🦊

### `init_rust_components`
- new component to provide Rust initialization routines, initially aimed at initializing NSS

- Our dependency on `rusqlite` was updated from 0.31.0 to 0.33.0. This crate uses `libsqlite3-sys`, and was also upgraded from 0.28.0 to 0.31.0. If you are using `libsqlite3-sys` in other parts of your dependency tree, you may have to upgrade it manually to continue compiling.

### Android
- Upgraded NDK from r27c to r28. ([#6588](https://github.com/mozilla/application-services/pull/6588))

### Glean
- Updated to v63.1.0 ([#6584](https://github.com/mozilla/application-services/pull/6584))

### Nimbus SDK ⛅️🔬🔭
- Enable enrollment status telemetry ([#6606](https://github.com/mozilla/application-services/pull/6606))
[Full Changelog](https://github.com/mozilla/application-services/compare/v136.0...v137.0)

# v136.0 (_2025-02-03_)

### Logins
The Logins component has been rewritten to use a newly introduced `EncryptorDecryptor` trait.

#### BREAKING CHANGE
The LoginsStore constructor and several API methods have been changed:

The signatures of the constructors are extended as follows:
```
pub fn new(path: impl AsRef<Path>, encdec: Arc<dyn EncryptorDecryptor>) -> ApiResult<Self>
pub fn new_from_db(db: LoginDb, encdec: Arc<dyn EncryptorDecryptor>) -> Self
pub fn new_in_memory(encdec: Arc<dyn EncryptorDecryptor>) -> ApiResult<Self>
```

The methods do not require an encryption key argument anymore, and return `Login` objects instead of `EncryptedLogin`:
```
pub fn list(&self) -> ApiResult<Vec<Login>>
pub fn get(&self, id: &str) -> ApiResult<Option<Login>>
pub fn get_by_base_domain(&self, base_domain: &str) -> ApiResult<Vec<Login>>
pub fn find_login_to_update(&self, entry: LoginEntry) -> ApiResult<Option<Login>>
pub fn update(&self, id: &str, entry: LoginEntry) -> ApiResult<Login>
pub fn add(&self, entry: LoginEntry) -> ApiResult<Login>
pub fn add_or_update(&self, entry: LoginEntry) -> ApiResult<Login>
```

New LoginsStore methods:
```
// Checking whether the database contains logins (does not utilize the `EncryptorDecryptor`):
is_empty(&self) -> ApiResult<bool>
// Checking for the Existence of Logins for a given base domain (also does not utilize the `EncryptorDecryptor`):
has_logins_by_base_domain(&self, base_domain: &str) -> ApiResult<bool>
```

The crypto primitives `encrypt`, `decrypt`, `encrypt_struct` and `decrypt_struct` are not exposed anymore via UniFFI, as well as `EncryptedLogin` will not be exposed anymore. In addition we also do not expose the structs `RecordFields`, `LoginFields` and `SecureLoginFields` anymore.


##### SyncEngine
The logins sync engine has been adapted for above EncryptorDecryptor trait and therefore does not support a `set_local_encryption_key` method anymore.

##### Flattened Login Struct
The flattened Login struct now does not expose internal structuring to the consumer:
```
Login {
    // record fields
    string id;
    i64 times_used;
    i64 time_created;
    i64 time_last_used;
    i64 time_password_changed;

    // login fields
    string origin;
    string? http_realm;
    string? form_action_origin;
    string username_field;
    string password_field;

    // secure login fields
    string password;
    string username;
}
```

### `rc_crypto`
- New low level bindings for dealing with primary password.
- New feature flag `keydb` in `rc_crypto/nss`, which enables NSS key persistence: `ensure_initialized_with_profile_dir(path: impl AsRef<Path>)` initializes NSS with a profile directory and appropriate flags to persist keys (and certificates) in its internal PKCS11 software implementation. This function must be called first; if `ensure_initialized` is called before, it will fail.
- New methods for dealing with primary password and key persistence, available within the `keydb` feature:
  * `authentication_with_primary_password_is_needed()`: checks whether a primary password is set and needs to be authenticated
  * `authenticate_with_primary_password(primary_password: &str)`: method for authenticate NSS key store against a user-provided primary password
  * `get_or_create_aes256_key(name: &str)`: retrieve a key by `name` from the internal NSS key store. If none exists, create one, persist, and return.

### Remote Settings
- Added support of content signatures verification ([#6534](https://github.com/mozilla/application-services/pull/6534))

[Full Changelog](https://github.com/mozilla/application-services/compare/v135.0...v136.0)

# v135.0 (_2025-01-06_)

## ✨ What's New ✨

### Nimbus SDK ⛅️🔬🔭
- Updated `getLocaleTag` to be publicly accessible ([#6510](https://github.com/mozilla/application-services/pull/6510))

## 🦊 What's Changed 🦊

### Glean
- Updated to v63.0.0 ([bug 1933939](https://bugzilla.mozilla.org/show_bug.cgi?id=1933939))

### Places
- `PlacesConnection::get_visited()` no longer considers unvisited-but-bookmarked pages to be visited, matching the behavior of both `get_visited_urls_in_range()` and Desktop ([#6527](https://github.com/mozilla/application-services/pull/6527)).

### `rc_crypto`
- New low level bindings for functions for key generation with persistence, for listing persisted keys, and for wrapping and unwrapping to get at the key material of stored keys.

## ⚠️ Breaking Changes ⚠️

## Webext-Storage
- Exposed the bridged engine logic for use in desktop ([#6473](https://github.com/mozilla/application-services/pull/6473)).
  - This updated the signature of the `bridged_engine` function technically making this PR a breaking change though an imminent desktop patch will remove references to function calls with the old signature.

[Full Changelog](https://github.com/mozilla/application-services/compare/v134.0...v135.0)

# v134.0 (_2024-11-25_)

## ✨ What's New ✨

### Relevancy
- Added init, select and update methods for Thompson Sampling (multi-armed bandit)

## 🦊 What's Changed 🦊

### Glean
- Updated to v62.0.0 ([bug 1928630](https://bugzilla.mozilla.org/show_bug.cgi?id=1928630))

### FxA Client
- Updated the iOS `sendToDevice` function to return the `closeTab` call's result when applicable. ([#6448](https://github.com/mozilla/application-services/pull/6448))

### Nimbus SDK ⛅️🔬🔭
- Added a standalone method to calculate targeting context attributes that are based on values in the Nimbus persistence layer ([#6493](https://github.com/mozilla/application-services/pull/6493))

### Places
- `PlacesConnection.noteHistoryMetadataObservation{ViewTime, DocumentType}()`
  (Android) and `PlacesWriteConnection.noteHistoryMetadataObservation()` (iOS)
  now take an optional `NoteHistoryMetadataObservationOptions` argument. The
  new `if_page_missing` option specifies what to do if the page for the
  observation doesn't exist in the history database.
  ([#6443](https://github.com/mozilla/application-services/pull/6443))

## ⚠️ Breaking Changes ⚠️

### Nimbus SDK ⛅️🔬🔭
- Added methods to `RecordedContext` for retrieving event queries and setting their values back to the foreign object ([#6322](https://github.com/mozilla/application-services/pull/6322)).

### Places
- If an entry for a page doesn't exist in the history database, any
  history observations for that page will no longer be recorded by default.
  To revert to the old behavior, and automatically insert an entry for
  the page before recording the observation, set the new `if_page_missing`
  option to `HistoryMetadataPageMissingBehavior::InsertPage`.

[Full Changelog](https://github.com/mozilla/application-services/compare/v133.0...v134.0)

# v133.0 (_2024-10-28_)

## ⚠️ Breaking Changes ⚠️

### Remote Settings
- Updated Error hierarchy.  We don't need to update consumer code because the only consumer was
  Android and it only caught exceptions using the base RemoteSettingsException class.
- Updated `RemoteSettingsServer::url()` to include the `v1/` path.  This makes the API match the JS
  version.  This only affects Rust consumers, since this function is not exposed via UniFFI.

## 🦊 What's Changed 🦊

### Android
- Upgraded NDK from r27 to r27c. ([#6432](https://github.com/mozilla/application-services/pull/6432))

### Glean
- Updated to v61.2.0 ([#6410](https://github.com/mozilla/application-services/pull/6410))

[Full Changelog](https://github.com/mozilla/application-services/compare/v132.0...v133.0)

# v132.0 (_2024-09-30_)

## ✨ What's New ✨

### General

- Simplified the process of adding a new component by adding a tool that can autogenerate the
  initial UniFFI/bindings code.

## 🦊 What's Changed 🦊

### Glean
- Updated to v61.1.0 ([#6397](https://github.com/mozilla/application-services/pull/6397))

[Full Changelog](https://github.com/mozilla/application-services/compare/v131.0...v132.0)

# v131.0 (_2024-08-30_)

## 🦊 What's Changed 🦊

### Glean
- Updated to v61.0.0 ([#6348](https://github.com/mozilla/application-services/pull/6348))

### Nimbus CLI
- Support for Firefox for Android and Focus v126+ (via gecko-dev).
  Fixed a bug where manifests for Firefox for Android v110 were not available, due
  to being fetched from the wrong repository
  ([#6347](https://github.com/mozilla/application-services/pull/6347))

[Full Changelog](https://github.com/mozilla/application-services/compare/v130.0...v131.0)

# v130.0 (_2024-08-05_)

## ✨ What's New ✨

### Suggest
- Added support for Fakespot suggestions.
- Added support for recording metrics
- Removed the `SuggestIngestionConstraints::max_suggestions` field.  No consumers were using this.

## 🦊 What's Changed 🦊

### Android
- Upgraded NDK from r26c to r27. ([#6305](https://github.com/mozilla/application-services/pull/6305))

### Glean
- Updated to v60.4.0 ([#6320](https://github.com/mozilla/application-services/pull/6320))

### Nimbus FML
- The output order should be deterministic again ([#6283](https://github.com/mozilla/application-services/pull/6283))

[Full Changelog](https://github.com/mozilla/application-services/compare/v129.0...v130.0)

# v129.0 (_2024-07-08_)

## ⚠️ Breaking Changes ⚠️

### Suggest
- Removed the deprecated `remote_settings_config` method.  No consumers were using this.

## ✨ What's New ✨

### Glean
- Updated to v60.3.0 ([#6279](https://github.com/mozilla/application-services/pull/6279))

### Suggest
- Added the `SuggestStoreBuilder.remote_settings_bucket_name` as a way to specify the bucket name.

[Full Changelog](https://github.com/mozilla/application-services/compare/v128.0...v129.0)

# v128.0 (_2024-06-10_)

## 🦊 What's Changed 🦊

### Glean
- Updated to v60.1.0 ([#6241](https://github.com/mozilla/application-services/pull/6241))

[Full Changelog](https://github.com/mozilla/application-services/compare/v127.0...v128.0)

# v127.0 (_2024-05-13_)

## 🦊 What's Changed 🦊

### General
- Updated minimum Python version to 3.8 ([#5961](https://github.com/mozilla/application-services/pull/5961)).

### Glean
- Updated to v60.0.0 ([#6209](https://github.com/mozilla/application-services/pull/6209))

[Full Changelog](https://github.com/mozilla/application-services/compare/v126.0...v127.0)

# v126.0 (_2024-04-15_)

## 🦊 What's Changed 🦊

### Nimbus SDK ⛅️🔬🔭

- Added `RecordedContext` extendable trait and define its usage in the Kotlin/Swift files ([#6207](https://github.com/mozilla/application-services/pull/6207)).

[Full Changelog](https://github.com/mozilla/application-services/compare/v125.0...v126.0)

# v125.0 (_2024-03-18_)

## 🦊 What's Changed 🦊

- The long-deprecated `rc_log` crate has been removed.

### Android
- Upgraded NDK from r25c to r26c. ([#6134](https://github.com/mozilla/application-services/pull/6134))

### Glean
- Updated to v58.1.0 ([#6150](https://github.com/mozilla/application-services/pull/6150)/[#6163](https://github.com/mozilla/application-services/pull/6163))

### Suggest
- Improved full keyword display for AMP suggestions
- `SuggestStoreBuilder.cache_path` is now deprecated because we no longer use the cache path.
- `SuggestStoreBuilder.remote_settings_config` is deprecated in favor of `remote_settings_server`, because `remote_settings_config` forced consumers that wanted to override the Remote Settings server to also specify the bucket and collection names.

### Remote Settings
- `RemoteSettingsConfig.server_url` is deprecated in favor of `server`, which is a `RemoteSettingsServer` instead of a string.
- The new `RemoteSettingsServer` type specifies the Remote Settings server to use.

[Full Changelog](https://github.com/mozilla/application-services/compare/v124.0...v125.0)

# v124.0 (_2024-02-15_)

## ✨ What's New ✨

### FxA Client
- Added a new API `setUserData` that sets the user's session token, to prevent session token duplication and allow User Agent applications to support a signed in, but not verified state. ([#6111](https://github.com/mozilla/application-services/pull/6111))

## 🦊 What's Changed 🦊

### Webext-Storage
- Uniffied the webext-storage component in preparation for desktop integration ([#6057](https://github.com/mozilla/application-services/pull/6057)).

### Remote Settings
- The Remote Settings UniFFI bindings have been changed. The
  `RemoteSettingsConfig` dictionary has had its field re-ordered to [fix code
  generation for the Python
  bindings](https://bugzilla.mozilla.org/show_bug.cgi?id=1874030). This also
  affects the Swift bindings, since Swift enforces argument ordering.

### Suggest
- Added more error variants to `SuggestApiError`
- Added `SuggestStoreBuilder` to create `SuggestStore` instances.
- `SuggestStore` now stores a data path.  This is the path to the SQLite database that should
  persist when the cache is cleared.

### Autofill
- Replace `*-name` fields with a single `name` field for addresses.
- Prevented outgoing syncs of scrubbed credit card records ([#6143](https://github.com/mozilla/application-services/pull/6143)).

## What's Fixed
- It was possible for sync to apply a tombstone for places while a bookmark was still in the database. This would have resulted in foreign constraint SQLite error.

[Full Changelog](https://github.com/mozilla/application-services/compare/v123.0...v124.0)

# v123.0 (_2024-01-22_)

## ✨ What's New ✨

### Nimbus FML ⛅️🔬🔭🔧

- Added correction candidates to errors returned by the FeatureInspector ([#6019](https://github.com/mozilla/application-services/pull/6019)).
  - This will drive the JSON editor on the Experimenter frontend.

### Nimbus SDK ⛅️🔬🔭

- Added definition for new Glean `nimbus-test-event` ([#6062](https://github.com/mozilla/application-services/pull/6062)).
  - Added `recordIsReady` method to NimbusInterface ([#6063](https://github.com/mozilla/application-services/pull/6063)).

## 🦊 What's Changed 🦊

### Nimbus CLI [⛅️🔬🔭👾](./components/support/nimbus-cli)
- Changed the locations of firefox-ios and focus-ios feature manifest files ([#6012](https://github.com/mozilla/application-services/pull/6012)) and added version sensitivity.

### Nimbus SDK ⛅️🔬🔭
- Moved the `days_since_install` calculation to closer to where it's needed ([#6042](https://github.com/mozilla/application-services/pull/6042)).
  - This means that the Nimbus SDK can run for longer and the JEXL evaluator still is accurate.

### Logins
- Logins now correctly handle the following sync conflict resolution:
   - When the client locally deleted a login, and before it synced another client modified the same login, the client will recover the login

### Tabs
- RemoteTabRecord now has an `inactive` boolean with a default value of false ([#6026](https://github.com/mozilla/application-services/pull/6026/)).
  Mobile platforms can populate this to indicate if the tab is "inactive" allowing other devices to treat them specially (eg, group them together, hide them by default, etc.)
### Push
- verifyConnection now returns an empty list when there are no subscriptions (which is typical when a device first starts up).

[Full Changelog](https://github.com/mozilla/application-services/compare/v122.0...v123.0)

# v122.0 (_2023-12-18_)

### 🦊 What's Changed 🦊

Bumped the version of rusqlite/libsqlite3-sys, meaning the bundled sqlite version is now 3.44.0.

## Nimbus FML ⛅️🔬🔭🔧

### ✨ What's New ✨

- Added an `info` command to add to the `nimbus-fml` command line ([#5967](https://github.com/mozilla/application-services/pull/5967)). It outputs JSON / YAML with a summary of each feature including:
  - the types used, as a proxy for feature complexity
  - [feature metadata](https://experimenter.info/fml/feature-metadata), including documentation and events
  - the schema hash and defaults hash.

## Places

### ⚠️ Breaking Changes ⚠️
  - Removed the `metrics_params` arguments from `begin_oauth_flow` and `begin_pairing_flow`.
    This is technically a breaking change, but no consumers were using these optional params so it shouldn't cause any issues downstream.

[Full Changelog](https://github.com/mozilla/application-services/compare/v121.0...v122.0)

# v121.0 (_2023-11-20_)

## Nimbus SDK ⛅️🔬🔭

### 🦊 What's Changed 🦊

- Removed `enrollment_id` from being generated or recorded anywhere in the Nimbus SDK ([#5899](https://github.com/mozilla/application-services/pull/5899)).
  - This was originally thought to be of use, but after running the system for sometime, we have found that this isn't needed.
  - In the spirit of reducing unique identifiers in telemetry and in the spirit of Lean Data, we have removed `enrollment_id` (and the code that generates it).
- Added the feature `activation` event ([#5908](https://github.com/mozilla/application-services/pull/5908)).

## Nimbus FML ⛅️🔬🔭🔧

### ✨ What's New ✨

- Added `string-alias` capability to feature variables ([#5928](https://github.com/mozilla/application-services/pull/5928)).
  - This adds quite a lot of type safety around complex features that relied on Strings, e.g. messaging, onboarding.

### 🦊 What's Changed 🦊

- FML errors are now sorted so that they are no longer non-deterministic ([#9741](https://github.com/mozilla/experimenter/issues/9741)).

## Places

### ⚠️ Breaking Changes ⚠️
  - `wipe_local` and `prune_destructively` have been removed from history API. `delete_everything` or `run_maintenance_*` methods should be used instead.

## FxA-Client

### 🦊 What's Changed 🦊

- Added the `LocalDevice` struct, tracks the server's knowledge of the local device.  Many
  device-related methods now return this.
- Check for missing sync scoped keys and return an error if they're not present
- Began implementing functionality to track the authorization state
- Added methods to simulate auth errors

[Full Changelog](https://github.com/mozilla/application-services/compare/v120.0...v121.0)

# v120.0 (_2023-10-23_)

## Nimbus SDK ⛅️🔬🔭

### ✨ What's New ✨

- Added the `enrollment_status` metric and defined a host metric callback handler interface ([#5857](https://github.com/mozilla/application-services/pull/5857)).

## Nimbus FML ⛅️🔬🔭🔧

### 🦊 What's Changed 🦊

- Changed `.experiment.yaml` generation to a `validate` ([#5877](https://github.com/mozilla/application-services/pull/5877)).
  - Additionally: moved `nimbus-fml.sh` script from iOS into the application-services directory, replacing it with a `bootstrap.sh` script.

## Rust log forwarder

### 🦊 What's Changed 🦊
- Exposed rust-log-forwarder for iOS ([#5840](https://github.com/mozilla/application-services/pull/5840)).
- Fixed rust-log-forwarder bindings for Focus iOS ([5858](https://github.com/mozilla/application-services/pull/5858)).

## Suggest
### ✨ What's New ✨

- Edits to AMO provider keyword matching to match keyword prefix

### ⚠️ Breaking Changes ⚠️

* The `include_sponsored` and `include_non_sponsored` Boolean options in `SuggestionQuery` have been replaced with a `providers` list. Consumers must now explicitly pass the providers they want to query ([#5867](https://github.com/mozilla/application-services/pull/5867)).
## Places

### ⚠️ Breaking Changes ⚠️

- VisitTransition has been renamed to VisitType to match Desktop and reduce the amount of conversions needed in consumer APIs.


### ✨ What's New ✨

- The `SuggestionQuery` now contains a optional limit that consumers can set to reduce the number of suggestions returned. ([#5870](https://github.com/mozilla/application-services/pull/5870))

[Full Changelog](https://github.com/mozilla/application-services/compare/v119.0...v120.0)

# v119.0 (_2023-09-25_)

## Nimbus SDK ⛅️🔬🔭

### ✨ What's New ✨

- The `set_experiments` method has been updated to filter down the list of experiments to only those that match the configured `app_name` and `channel` ([#5813](https://github.com/mozilla/application-services/pull/5813)).

## Nimbus FML ⛅️🔬🔭🔧

### ✨ What's New ✨

- Added new optional metadata fields to a feature definition ([#5865](https://github.com/mozilla/application-services/pull/5865)).
  - This is to open up uses of the FML in experimenter, especially for QA and feature configuration.
- Added support to fetch different versions of a manifest to the `FmlClient` ([#5827](https://github.com/mozilla/application-services/pull/5827)).
- Added a `FmlFeatureInspector` to the `FmlClient` ([#5827](https://github.com/mozilla/application-services/pull/5827)).
  - This adds methods to parse a feature configuration and return errors.
- Added a `FmlFeatureDescriptor` to the `FmlClient` ([#5815](https://github.com/mozilla/application-services/pull/5815)).
  - This adds methods to get the feature_ids and descriptions from a loaded manifest.
- Added a `channels` subcommmand to the command line ([#5844](https://github.com/mozilla/application-services/pull/5844)).
  - This prints the channels for the given manifest.
- Added `pref-key` to feature variables schema definition ([#5862](https://github.com/mozilla/application-services/pull/5862)).
  - This allows developers to override remote and default values of top-level variables with preferences.
  - Requires setting `userDefaults` and `sharedPreferences` in the call to `NimbusBuilder`.

### 🦊 What's Changed 🦊

- Removed the `channel` argument from the `generate-experimenter` command ([#5843](https://github.com/mozilla/application-services/pull/5843)).
  - This cleans up some design issues/technical debt deep within the internal representation of the FML compiler.

## Places

### 🦊 What's Changed 🦊

- `fetch_tree_with_depth` no longer starts a transaction when reading from the database. The transaction causes issues with concurrent calls, and isn't needed for consistency anymore ([#5790](https://github.com/mozilla/application-services/pull/5790)).

## Suggest

### ✨ What's New ✨

- The Suggest component now has Swift bindings for Firefox for iOS ([#5806](https://github.com/mozilla/application-services/pull/5806)).

### 🦊 What's Changed 🦊

- AMP suggestions now replace all template parameters in their `url` and `click_url` fields, and carry the original "raw" URLs in the `raw_url` and `raw_click_url` fields. Consumers can use the `raw_suggestion_url_matches()` function to determine if a `raw_url` or `raw_click_url` matches a URL string with replacements. This is a source-breaking change in Swift only, and doesn't affect Firefox for iOS, because iOS isn't consuming the Suggest component yet ([#5826](https://github.com/mozilla/application-services/pull/5826)).

## Logins

### ⚠️ Breaking Changes ⚠️

- Removal of SQLCipher migration API call and the SQLCipher library. While not technically a breaking change, it's listed here given that we're removing a large library.

[Full Changelog](https://github.com/mozilla/application-services/compare/v118.0...v119.0)

# v118.0 (_2023-08-28_)

## General
### 🦊 What's Changed 🦊

- Backward-incompatible changes to the Suggest database schema to accommodate custom details for providers ([#5745](https://github.com/mozilla/application-services/pull/5745)) and future suggestion types ([#5766](https://github.com/mozilla/application-services/pull/5766)). This only affects prototyping, because we aren't consuming Suggest in any of our products yet.
- The `Suggestion` type in the Suggest component has changed from a dictionary to an enum ([#5766](https://github.com/mozilla/application-services/pull/5766)). This only affects prototyping, because we aren't consuming Suggest in any of our products yet.
- The Remote Settings `Client::get_attachment()` method now returns a `Vec<u8>` instead of a Viaduct `Response` ([#5764](https://github.com/mozilla/application-services/pull/5764)). You can use the new `Client::get_attachment_raw()` method if you need the `Response`. This is a backward-incompatible change for Rust consumers only; Swift and Kotlin are unaffected.
- The Remote Settings client now parses `ETag` response headers from Remote Settings correctly ([#5764](https://github.com/mozilla/application-services/pull/5764)).

### ✨ What's New ✨

- Added an OHTTP client library for iOS based on `ohttp` Rust crate ([#5749](https://github.com/mozilla/application-services/pull/5749)). This allows iOS products to use the same OHTTP libraries as Gecko-based products.
- The Remote Settings client has a new `Client::get_records_with_options()` method ([#5764](https://github.com/mozilla/application-services/pull/5764)). This is for Rust consumers only; it's not exposed to Swift or Kotlin.
- `RemoteSettingsRecord` objects have a new `deleted` property that indicates if the record is a tombstone ([#5764](https://github.com/mozilla/application-services/pull/5764)).
- Added `server-megazord` build for compiling crates and uniffi-ing for use in a python based server ([#5804](https://github.com/mozilla/application-services/pull/5804)).
  - Initial users of this are a new `nimbus-experimenter` megazord and the existing `cirrus` megazord.

## Rust log forwarder
### 🦊 What's Changed 🦊

- Renamed `Logger` to `AppServicesLogger` to avoid a name conflict on Swift.

## Nimbus CLI [⛅️🔬🔭👾](./components/support/nimbus-cli)

### ✨ What's New ✨

- Added passthrough to FML command line ([#5784](https://github.com/mozilla/application-services/pull/5784)), effectively unifying the two command line tools.

## Nimbus FML ⛅️🔬🔭🔧

### 🦊 What's Changed 🦊

- Removed previously deprecated commands `experimenter`, `ios`, `android`, `intermediate-repr` ([#5784](https://github.com/mozilla/application-services/pull/5784)).

[Full Changelog](https://github.com/mozilla/application-services/compare/v117.0...v118.0)

# v117.0 (_2023-07-31_)

## General

### 🦊 What's Changed 🦊

- Removed obsolete sync functions that were exposed for Firefox iOS prior to the sync manager component integration ([#5725](https://github.com/mozilla/application-services/pull/5725)).

### ✨ What's New ✨

- Added a new Firefox Suggest component ([#5723](https://github.com/mozilla/application-services/pull/5723)).

## Nimbus SDK ⛅️🔬🔭

### ✨ What's New ✨

- Add `recordExperimentExposure` to `FeatureHolder`, and substitute `{experiment}` for experiment slugs at enrollment in the feature configuration [#5715](https://github.com/mozilla/application-services/pull/5715).
  - This is to enable exposure events to be assigned to the correct experiment in coenrolled features.
  - Android and iOS are both supported.

## Nimbus FML ⛅️🔬🔭🔧

### ✨ What's New ✨

- Add `allow-coenrollment` property for features in the Feature Manifest Language. This relaxes the feature exclusion rules for features marked with `allow-coenrollment: true`. ([#5688](https://github.com/mozilla/application-services/pull/5688)).
  - This adds a non-user-facing method to the `FeatureManifestInterface`, `getCoenrollingFeatureIds`, in both Kotlin and Swift.
- Exposes a method to get the coenrolling feature ids in the FML client ([#5714](https://github.com/mozilla/application-services/pull/5714)), as well as the NimbusBuilders for both Kotlin and Swift ([#5718](https://github.com/mozilla/application-services/pull/5718)).

### 🦊 What's Changed 🦊

- Make sure deterministic builds of downstream consumers are not broken ([#5736](https://github.com/mozilla/application-services/pull/5736)).

## Nimbus CLI [⛅️🔬🔭👾](./components/support/nimbus-cli)

### ✨ What's New ✨

- Updated the version number to 0.4.0 ([#5757](https://github.com/mozilla/application-services/pull/5757)).
- Added a `--patch` option to all commands that accept an experiment. ([#5721](https://github.com/mozilla/application-services/pull/5721))
- Added a `--pbpaste` and `start-server` command for testing on iOS devices. ([#5751](https://github.com/mozilla/application-services/pull/5751)).
  - Use by `start-server` which directs you to open a URL on your device. Device commands sync with the server, and then on to the device.
  - Added a `--pbcopy` option to all commands that open the app. These URLs are used to open the app on device ([#5727](https://github.com/mozilla/application-services/pull/5727)).
    - with associated in-app tooling to enroll into experiments via a deeplink URL.
  - Added `--is-launcher` to the protocol between the cli and the apps, so apps detecting the launcher intent can work from the generated deeplinks ([#5748](https://github.com/mozilla/application-services/pull/5748)).
- Added filters to the `list` and `fetch-list` commands ([#5730](https://github.com/mozilla/application-services/pull/5730))
  - Also, made `--app` and `--channel` non-mandatory for commands that don't need them.

[Full Changelog](https://github.com/mozilla/application-services/compare/v116.0...v117.0)

# v116.0 (_2023-07-03_)

## General

### 🦊 What's Changed 🦊
- Android: The JVM compatibility target is now version 17 ([#5651](https://github.com/mozilla/application-services/pull/5651))
  - _NOTE: This is technically a breaking change, but all existing downstream projects have already made the necessary changes._

## Nimbus SDK ⛅️🔬🔭

### 🦊 What's Changed 🦊

- When a rollout audience size changes, the enrollment is re-evaluated and the client is un-enrolled if no longer in the bucket or re-enrolled if they had previously been disqualified from bucketing. ([#5687](https://github.com/mozilla/application-services/pull/5687), [#5716](https://github.com/mozilla/application-services/pull/5716)).
  - The record of enrollment will be available in the new `enrollments` targeting attribute.
- Add `enrollments` value to `TargetingAttributes` — it is a set of strings containing all enrollments, past and present ([#5685](https://github.com/mozilla/application-services/pull/5685)).
  - _Note: This change only applies to stateful uses of the Nimbus SDK, e.g. mobile_
- Add ability to enroll selected features multiple times (coenrollment) ([#5684](https://github.com/mozilla/application-services/pull/5684), [#5697](https://github.com/mozilla/application-services/pull/5697)).

### ⚠️ Breaking Changes ⚠️

- Several changes to the NimbusBuilder mean that this is a breaking change for Firefox for Android [#5697](https://github.com/mozilla/application-services/pull/5697).
  - These changes are fixed by [firefox-android#2682](https://github.com/mozilla-mobile/firefox-android/pull/2682).

## Nimbus FML ⛅️🔬🔭🔧

### ✨ What's New ✨

- Add `validate` command to the FML CLI. This command validates a chosen manifest file, including all its imports, includes, and channels ([#5607](https://github.com/mozilla/application-services/pull/5607)).
- Add `single-file` command to the FML CLI. This command rationalizes a manifest file– including all of its imports and includes– into a single file, suitable for bundling into a secure environment ([#5676](https://github.com/mozilla/application-services/pull/5676)).

### 🦊 What's Changed 🦊

- When a cache directory is not specified, now spin up a temporary directory instead of using the (sometimes) long lived system one ([#5662](https://github.com/mozilla/application-services/pull/5662)).

## Nimbus CLI [⛅️🔬🔭👾](./components/support/nimbus-cli)

### 🦊 What's Changed 🦊

- Version bump to `0.3.0` ([#5674](https://github.com/mozilla/application-services/pull/5674)). User facing changelog: https://experimenter.info/nimbus-cli/whats-new
- Added isRollout and bucketing info to the `list` command ([#5672](https://github.com/mozilla/application-services/pull/5672)).
- Fixed several paper cut usability issues ([#5654](https://github.com/mozilla/application-services/pull/5654)):
  - Experiments by default are fetched from the API v6, eliminating latency between making changes on experimenter and syncing with remote settings.
  - Separated `fetch` and `fetch-list`: experiment lists, by default still come from Remote Settings, but the slower API v6 `/api/v6/experiments` can be queried.

### ✨ What's New ✨

- Added an `info` command ([#5672](https://github.com/mozilla/application-services/pull/5672)).
- Fixed several paper cut usability issues ([#5654](https://github.com/mozilla/application-services/pull/5654)):
  - Added a `defaults` command to output the feature configuration from the manifest.
  - Added a `features` command to output the experiment branch features and optionally merged with the manifest defaults.
  - Now supports reading and writing YAML files.
  - Single experiment files can be used as an experiment list file.
- Add passthrough parameters for the `open`, `enroll` and `test-feature` commands. ([#5669](https://github.com/mozilla/application-services/pull/5669))

[Full Changelog](https://github.com/mozilla/application-services/compare/v115.0...v116.0)

# v115.0 (_2023-06-05_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v114.0...v115.0)

## Push
### ⚠️ Breaking Changes ⚠️
  - The constructor for the Push Manager has changed. ([#5389](https://github.com/mozilla/application-services/pull/5389))
    - Push manager now takes only one argument, a Push Configuration object
    - Push manager no longer takes in the registration_id (token) in construction
    - Push manager now takes a new `verifyConnectionRateLimiter` parameter in its configuration, it defines the number of seconds between consecutive verify connection requests.
  - The `update` function no longer returns a boolean, the consumers did not use the return value. ([#5389](https://github.com/mozilla/application-services/pull/5389))
  - The Error exposed by push is now `PushApiError`, which is reduced to the set of errors the consumer is expected to handle. ([#5389](https://github.com/mozilla/application-services/pull/5389)):
     - `PushApiError::UAIDNotRecognizedError`: The server lost the client's uaid. The app should call `verify_connection(true)` and notify all consumers of push
     - `RecordNotFoundError`: The record containing the private key cannot be found. The consumer should call `verify_connection(true)` and notify all consumers of push
     - `InternalError`: Consumer should report the error, but ignore it

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊

- Add additional Cirrus SDK helper methods and add Python testing for the generated Cirrus Python code ([#5478](https://github.com/mozilla/application-services/pull/5478)).
- Add `user_id` RandomizationUnit (for Cirrus) ([#5564](https://github.com/mozilla/application-services/pull/5564)).
- Fixed up a bug in `get_experiment_branch` and `get_active_experiments` ([(#5584)](https://github.com/mozilla/application-services/pull/5584)).
- Renamed `GleanPlumb` classes and protocols to `NimbusMessaging` in Swift. Added more protocols to make it more mockable in application code ([#5604](https://github.com/mozilla/application-services/pull/5604)).

## Nimbus CLI [⛅️🔬🔭👾](./components/support/nimbus-cli)

### ✨ What's New ✨

- Extra commands: `capture-logs`, `tail-logs`, `test-feature`, `fetch` and `apply-files`. ([#5517](https://github.com/mozilla/application-services/pull/5517))
- Extra commands: `validate` to validate experiments against a feature manifest. ([#5638](https://github.com/mozilla/application-services/pull/5638))
  - This is on by default for `test-feature` and `enroll`
  - The version of the manifest may be tweaked with the `--version`, `--ref` and `--manifest` options.
- Extra commands and options to open deeplinks with the app. ([#5590](https://github.com/mozilla/application-services/pull/5590)).
- An update checker to keep you and your installation fresh. ([#5613](https://github.com/mozilla/application-services/pull/5613)).
- An installation script to make getting `nimbus-cli` easier. ([#5618](https://github.com/mozilla/application-services/pull/5618)).

## Nimbus FML ⛅️🔬🔭🔧

### ✨ What's New ✨

- Added a new Remote Settings client component ([#5423](https://github.com/mozilla/application-services/pull/5423)).
- Added `toJSONObject()` and `getFeatures(featureId)` for Kotlin. This serializes the FML into a `JSONObject` ([#5574](https://github.com/mozilla/application-services/pull/5574)).
- Added `FmlClient`, additional methods to `FeatureManifest`, and Python UniFFI bindings ([#5557](https://github.com/mozilla/application-services/pull/5557)).
- Updated FML validation to error when a property name that doesn't exist is applied to a feature ([#5620](https://github.com/mozilla/application-services/pull/5620)).

### What's fixed

- Updated the FML downloader plugin to correctly download the release artifacts from the GitHub release

## FxA Client
### ⚠️ Breaking Changes ⚠️
- Changes `handlePushMessage` API so that it now returns exactly one event associated with the push message. ([#5556](https://github.com/mozilla/application-services/pull/5556))
   - This API does not attempt to retrieve any missing tabs.
   - Users of this API can use it to display the notification, but should use `pollDeviceCommands` after to capture the commands.

## Xcode

- Bumped Xcode version from 13.4.1 -> 14.3.1 ([#5615](https://github.com/mozilla/application-services/pull/5615))

# v114.0 (_2023-05-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.5.1...v114.0)

## General

### ✨ What's New ✨
  - Updated the version system to follow the Firefox major version
  - Implemented nightly builds

## Nimbus ⛅️🔬🔭

### ✨ What's New ✨

- Added processing of command line arguments (or intent extras) to be driven by a command line tool. ([#5482](https://github.com/mozilla/application-services/pull/5482))
  - Requires passing `CommandLine.arguments` to `NimbusBuilder` in iOS.
  - Requires passing `intent` to `NimbusInterface` in Android.
- Added Cirrus client object for working with Nimbus in a static, stateless manner ([#5471](https://github.com/mozilla/application-services/pull/5471)).
- Added [`nimbus-cli`](./components/support/nimbus-cli). ([#5494](https://github.com/mozilla/application-services/pull/5494))

## Places ⛅️🔬🔭

### 🦊 What's Changed 🦊

  - Added support for sync payload evolution in history.  If other clients sync history records / visits with fields that we don't know about, we store that data as JSON and send it back when it's synced next.

# v97.5.1 (_2023-04-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.5.0...v97.5.1)

## General

### ✨ What's New ✨
  - Fixing the objcopy path when building the megazord ([#5154](https://github.com/mozilla/application-services/pull/5154)).

# v97.5.0 (_2023-04-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.4.1...v97.5.0)

## General

### What's Changed

- Android: Upgraded NDK from r21d to r25c.

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊
- Refactor the `EnrollmentEvolver` in preparation for a larger refactor to split out the `stateful` feature. ([#5374](https://github.com/mozilla/application-services/pull/5374)).
- Added a `stateful` cargo feature and added appropriate feature flag attributes ([#5448](https://github.com/mozilla/application-services/pull/5448)).
  - This does not functionally change build processes, as the `stateful` feature is now the default feature for the `nimbus-sdk` library.
- Changed the ordering around for optional arguments for Python compatibility ([#5460](https://github.com/mozilla/application-services/pull/5460)).
  - This does not change Kotlin or Swift APIs, but affects code that uses the uniffi generated FFI for `record_event` and `record_past_event` directly.
### ✨ What's New ✨

- Added more testing tools for the `NimbusEventStore`, for iOS and Android ([#5477](https://github.com/mozilla/application-services/pull/5477))
  - `events.advanceEventTime(by: time)` lets you queue up a sequence of events to test JEXL queries.

## Sync Manager

### 🦊 What's Changed 🦊
  - Added the sync telemetry reporting logic to replace the temp metrics in iOS. ([#5479](https://github.com/mozilla/application-services/pull/5479))

# v97.4.1 (_2023-04-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.4.0...v97.4.1)

## Places

### 🦊 What's Changed 🦊
  - Added a workaround for a database migration issue that was breaking places for some nightly users
    (https://github.com/mozilla/application-services/issues/5464)

# v97.4.0 (_2023-04-03_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.3.0...v97.4.0)

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊

- Changed the ordering around for optional arguments for Python compatibility ([#5460](https://github.com/mozilla/application-services/pull/5460)).
  - This does not change Kotlin or Swift APIs, but affects code that uses the uniffi generated FFI for `record_event` and `record_past_event` directly.

# v97.3.0 (_2023-03-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.2.0...v97.3.0)

## Places ⛅️🔬🔭

### 🦊 What's Changed 🦊

  - Added support for sync payload evolution in bookmarks.  If other clients sync bookmark records with fields that we don't know about, we store that data as JSON and send it back when it's synced next.

## Nimbus ⛅️🔬🔭

### ✨ What's New ✨

  - Added `recordPastEvent` for iOS and Android for testing of event store triggers. ([#5431](https://github.com/mozilla/application-services/pull/5431))
  - Added `recordMalformedConfiguration` method for `FeatureHolder` to record when some or all of a feature configuration is found to be invalid. ([#5440](https://github.com/mozilla/application-services/pull/5440))

### 🦊 What's Changed 🦊

  - Removed the check for major `schemaVersion` in Experiment recipes. ([#5433](https://github.com/mozilla/application-services/pull/5433))

# v97.2.0 (_2023-03-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.1.0...v97.2.0)

## General

### 🦊 What's Changed 🦊
- Android: The JVM compatibility target is now version 11 ([#5401](https://github.com/mozilla/application-services/issues/5401))
  - _NOTE: This is technically a breaking change, but all existing downstream projects have already made the necessary changes._

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊
- Fix Nimbus gradle plugin source file and task dependency issues ([#5421](https://github.com/mozilla/application-services/pull/5421))

### ✨ What's New ✨
- Added new testing tooling `HardcodeNimbusFeatures` to aid UI and integration tests ([#5393](https://github.com/mozilla/application-services/pull/5393))

# v97.1.0 (_2023-02-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v97.0.0...v97.1.0)

## Tabs

### 🦊 What's Changed 🦊

- The Tabs engine now trims the payload to be under the max the server will accept ([#5376](https://github.com/mozilla/application-services/pull/5376))


## Sync Manager

### 🦊 What's Changed 🦊

- Exposing the Sync Manager component to iOS by addressing the existing naming collisions, adding logic to process the telemetry
  data returned in the component's `sync` function, and adding the component to the iOS megazord ([#5359](https://github.com/mozilla/application-services/pull/5359)).

# v97.0.0 (_2023-02-22_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.4.0...v97.0.0)

## Nimbus ⛅️🔬🔭
### 🦊 What's Changed 🦊
- Updated the Nimbus Gradle Plugin to fix a number of issues after migrating it to this repository ([#5348](https://github.com/mozilla/application-services/pull/5348))
- Good fences: protected calls out to the error reporter with a `try`/`catch` ([#5366](https://github.com/mozilla/application-services/pull/5366))
- Updated the Nimbus FML CLI to only import the R class if it will be used by a feature property ([#5361](https://github.com/mozilla/application-services/pull/5361))
### ⚠️ Breaking Changes ⚠️
  - Android and iOS: Several errors have been moved to an internal support library and will no longer be reported as top-level Nimbus errors. They should still be accessible through `NimbusError.ClientError`. They are: `RequestError`, `ResponseError`, and `BackoffError`. ([#5369](https://github.com/mozilla/application-services/pull/5369))

# v96.4.0 (_2023-01-30_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.3.0...v96.4.0)

## Tabs

### What's Changed

The Tabs engine is now more efficient in how it fetches its records:

- The Tabs engine no longer clears the DB on every sync.
- Tabs now tracks the last time it synced and only fetches tabs that have changed since the last sync.
- Tabs will keep records for up to 180 days, in parity with the clients engine. To prevent the DB from getting too large.

## Nimbus ⛅️🔬🔭

### 🦊 What's Changed 🦊
  - Added `GleanMetrics.NimbusHealth` metrics for measuring duration of `apply_pending_experiments` and `fetch_experiments`. ([#5344](https://github.com/mozilla/application-services/pull/5344))

# v96.3.0 (_2023-01-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.2.1...v96.3.0)

## Places
### What's changed
 - Removes old iOS bookmarks migration code. The function `migrateBookmarksFromBrowserDb` no longer exists. ([#5276](https://github.com/mozilla/application-services/pull/5276))

## Nimbus ⛅️🔬🔭
### What's New
  - iOS: added a `Bundle.fallbackTranslationBundle()` method. ([#5314](https://github.com/mozilla/application-services/pull/5314))
  - Moved the Nimbus Gradle Plugin into application-services and updated its functionality to support local development. ([#5173](https://github.com/mozilla/application-services/pull/5173))

# v96.2.1 (_2023-01-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.2.0...v96.2.1)

## Places
### What's changed
  - Limited the number of visits to migrate for History to 10,000 visits. ([#5310](https://github.com/mozilla/application-services/pull/5310))

# v96.2.0 (_2023-01-03_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.1.3...v96.2.0)

## Nimbus ⛅️🔬🔭

### What's Changed
  - Fixed an issue where the NimbusQueues protocol was missing from the NimbusApi ([#5298](https://github.com/mozilla/application-services/pull/5298))

### ✨ What's New ✨
  - Added `eventLastSeen` query to the event store and jexl transforms ([#5297](https://github.com/mozilla/application-services/pull/5297))
  - Introduced the `NimbusBuilder` for Swift ([#5307](https://github.com/mozilla/application-services/pull/5307))

## Tabs

### What's Changed
  - Fixed a regression causing failure to read old tabs databases ([#5286](https://github.com/mozilla/application-services/pull/5286))

## Autofill

### 🦊 What's Changed 🦊
  - Exposed autofill api to iOS consumer and resolved `createKey` function conflict with the Logins component.

# v96.1.3 (_2022-12-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.1.2...v96.1.3)

## Tabs

### What's Changed
  - Fixed a regression causing failure to read old tabs databases ([#5286](https://github.com/mozilla/application-services/pull/5286))

# v96.1.2 (_2022-12-07_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.1.1...v96.1.2)

## Logins
### What's changed
 - Removes Fennec migration code. The function `importMultiple` no longer exists. ([#5268](https://github.com/mozilla/application-services/pull/5268))

## Nimbus

### What's Changed
  - Event store date comparison logic update to be entirely relative ([#5265](https://github.com/mozilla/application-services/pull/5265))
  - Updates event store to initialize all dates at the start of the current year ([#5279](https://github.com/mozilla/application-services/pull/5279))
  - Adds new Kotlin/Swift methods to clear the event store ([#5279](https://github.com/mozilla/application-services/pull/5279))
  - Adds Swift methods to wait for operation queues to finish ([#5279](https://github.com/mozilla/application-services/pull/5279))

## Places
### What's changed
 - Removes Fennec migration code. ([#5268](https://github.com/mozilla/application-services/pull/5268))
  The following functions no longer exist:
   - `importBookmarksFromFennec`
   - `importPinnedSitesFromFennec`
   - `importVisitsFromFennec`

## Viaduct
### What's New
  - Allow viaduct to make requests to the android emulator's host address via
    a new viaduct_allow_android_emulator_loopback() (in Rust)/allowAndroidEmulatorLoopback() (in Kotlin)
    ([#5270](https://github.com/mozilla/application-services/pull/5270))

## Tabs
### What's changes
  - The ClientRemoteTabs struct/interface now has a last_modified field which is the time
    when the device last uploaded the tabs.

# v96.1.1 (_2022-12-01_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.1.0...v96.1.1)

## autofill

### What's Changed
  - Fixed a bug where `scrub_encrypted_data()` didn't update the last sync time, which prevented the scrubbed CC data
    from being fixed.
  - Don't report sentry errors when we try to decrypt the empty string.  This happens when the consumer tries to decript
    a CC number after `scrub_encrypted_data()` is called.

## logins

### What's Changed
  -  Don't report `Origin is Malformed` errors to Sentry.  This is a known issue stemming from FF Desktop sending us
     URLs without a scheme.  See #5233 for details.

## places

### What's Changed
  - Switch to using incremental vacuums for maintenance, which should speed up the process.
  - Don't report places `relative URL without a base` to Sentry.  This is a known issue caused by Fenix sending us URLs
    with an invalid scheme (see #5235)

# v96.1.0 (_2022-11-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.0.1...v96.1.0)

## FxA Client

### What's new
- Exposed a new function for swift consumers `resetPersistedState`
   - `resetPersistedState` can be used to refresh the account manager to reflect the latest persisted state.
   - `resetPersistedState` should be called in between a different account manager instance persisting the state, and the current account manager persisting state
     - For example, the Notification Service in iOS creates its own instance of the account manager, changes its state (by changing the index of the last retrieved send tab)
     - The main account manager held by the application should call` resetPersistedState` before calling any other method that might change its state. This way it can retrieve the most up to date index that the Notification Services persisted.
### What's changed
- The `processRawIncomingAccountEvent` function will now process all commands, not just one. This moves the responsibility of ensuring each push gets a UI element to the caller.

# v96.0.1 (_2022-11-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v96.0.0...v96.0.1)

## Logins

### What's Changed
  - Updated the URL redaction code to remove potential PII leak.  Version `96.0.0` should not be used by downstream clients.

## Nimbus
### What's changed
- Add methods to Kotlin and Swift to call the record event method on the nimbus client ([#5244](https://github.com/mozilla/application-services/pull/5244))

## FxA Client
### What's changed
- The devices retrieved from the devices list are now only the devices that have been accessed in 21 days. This should help remove duplicates and idle devices for users. ([#4984](https://github.com/mozilla/application-services/pull/4984))

# v96.0.0 (_2022-11-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v95.0.1...v96.0.0)

## ⛅️🔬🔭 Nimbus

### ✨ What's New ✨
  - `active_experiments` is available to JEXL as a set containing slugs of all enrolled experiments ([#5227](https://github.com/mozilla/application-services/pull/5227))
  - Added Behavioral Targeting/Display Triggers accessible from JEXL for experiments and messages ([#5226](https://github.com/mozilla/application-services/pull/5226), [#5228](https://github.com/mozilla/application-services/pull/5228))
  - Android only: added a new `NimbusBuilder` method to unify Fenix and Focus startup sequences. ([5239](https://github.com/mozilla/application-services/pull/5239))

### ⚠️ Breaking Changes ⚠️
  - Changed the type of `customTargetingAttributes` in `NimbusAppSettings` to a `JSONObject`. The change will be breaking only for Android. ([#5229](https://github.com/mozilla/application-services/pull/5229))
  - Android only: Removed the `initialize()` methods in favor of `NimbusBuilder` class. ([5239](https://github.com/mozilla/application-services/pull/5239))

## Logins

### What's Changed
  - Include a redacted version of the URL in the Sentry error report when we see a login with an invalid origin field.
  - Made it so `InvalidDatabaseFile` errors aren't reported to Sentry.  These occurs when a non-existent path is passed
    to `migrateLoginsWithMetrics()`, which happens about 1-2 times a day.  This is very low volume, the code is going
    away soon, and we have a plausible theory that these happen when Fenix is killed after the migration but before
    `SQL_CIPHER_MIGRATION` is stored.

## Places

### What's Changed
  - Report a Sentry breadcrumb when we fail to parse URLs, with a redacted version of the URL.

## JwCrypto

### What's Changed
  - Log a breadcrumb with a redacted version of the crypto key when it has an invalid form (before throwing
    DeserializationError)

# v95.0.1 (_2022-11-03_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v95.0.0...v95.0.1)

# General
  - Added function to unset the app-services error reporter

# v95.0.0 (_2022-10-28_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.3.2...v95.0.0)

## General
### What's fixed
- Fixed a bug released in 94.3.1. The bug broke firefox-ios builds due to a name conflict. ([#5181](https://github.com/mozilla/application-services/pull/5181))

### What's Changed
  - Updated UniFFI to 0.21.0.  This improves the string display of the fielded errors on Kotlin.  Currently only logins is using these errors, but we plan to start using them for all components.

## Autofill

### ⚠️ Breaking Changes ⚠️

   - The autofill API now uses `AutofillApiError` instead of `AutofillError`.   `AutofillApiError` exposes a smaller number of variants, which
     will hopefully make it easier to use for the consumer.

## Logins

### ⚠️ Breaking Changes ⚠️

   - Renamed `LoginsStorageError` to `LoginsApiError`, which better reflects how it's used and makes it consistent with
     the places error name.
   - Removed the `LoginsApiError::RequestFailed` variant.  This was only thrown when calling the sync-related methods
     manually, rather than going through the SyncManager which is the preferred way to sync. Those errors will now be
     grouped under `LoginsApiError::UnexpectedLoginsApiError`.

### What's Changed
  - Added fields to errors in `logins.udl`.  Most variants will now have a `message` field.

## Nimbus ⛅️🔬🔭

### What's Changed
  - Disabled Glean events recorded when the SDK is not ready for a feature ([#5185](https://github.com/mozilla/application-services/pull/5185))
  - Add structs for behavioral targeting ([#5205](https://github.com/mozilla/application-services/pull/5205))
  - Calls to `log::error` have been replaced with `error_support::report_error` ([#5204](https://github.com/mozilla/application-services/pull/5204))

## Places

### ⚠️ Breaking Changes ⚠️

   - Renamed `PlacesError` to `PlacesApiError`, which better reflects that it's used in the public API rather than for
     internal errors.
   - Removed the `JsonError`, `InternalError`, and `BookmarksCorruption` variants from places error. Errors that
     resulted in `InternalError` will now result in `UnexpectedPlacesError`. `BookmarksCorruption` will also result in
     an `UnexpectedPlacesError` and an error report will be automatically generated. `JsonError` didn't seem to be
     actually used.

## Tabs

### ⚠️ Breaking Changes ⚠️

   - The tabs API now uses  `TabsError` with `TabsApiError`.  `TabsApiError` exposes a smaller number of variants, which
     will hopefully make it easier to use for the consumer.

# v94.3.2 (_2022-10-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.3.1...v94.3.2)

## General

### What's Changed

- Android: Reverted NDK back to r21d from r25b. ([#5156](https://github.com/mozilla/application-services/issues/5165))

## Sync Manager

### What's Changed
  - Syncing will sync each engine in a deterministic order which matches desktop ([#5171](https://github.com/mozilla/application-services/issues/5171))

# v94.3.1 (_2022-09-23_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.3.0...v94.3.1)

## Nimbus ⛅️🔬🔭

### What's Fixed

   - A regression affecting Android in calculating `days_since_install` and `days_since_update` ([#5157](https://github.com/mozilla/application-services/pull/5157))

# v94.3.0 (_2022-09-20_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.2.0...v94.3.0)

## General

### What's Changed

- Rust toolchain has been bumped to 1.63 and minimum version bumped to 1.61 to comply with our [Rust Policy](https://github.com/mozilla/application-services/blob/main/docs/rust-versions.md#application-services-rust-version-policy)
- Android: Upgraded NDK from r21d to r25b. ([#5142](https://github.com/mozilla/application-services/pull/5142))

## Places

### What's Changed
 - Added metrics for the `run_maintenance()` method.  This can be used by consumers to decide when to schedule the next `run_maintenance()` call and to check if calls are taking too much time.

### What's new
  - Exposed a function in Swift `migrateHistoryFromBrowserDb` to migrate history from `browser.db` to `places.db`, the function will migrate all the local visits in one go. ([#5077](https://github.com/mozilla/application-services/pull/5077)).
    - The migration might take some time if a user had a lot of history, so make sure it is **not** run on a thread that shouldn't wait.
    - The migration runs on a writer connection. This means that other writes to the `places.db` will be delayed until the migration is done.

## Nimbus

### What's Changed
 - Added `applyLocalExperiments()` method as short hand for `setLocalExperiments` and `applyPendingExperiments`. ([#5131](https://github.com/mozilla/application-services/pull/5131))
   - `applyLocalExperiments` and `applyPendingExperiments` now returns a cancellable job which can be used in a timeout.
   - `initialize` function takes a raw resource file id, and returns a cancellable `Job`.

# v94.2.1 (_2022-09-21_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.2.0...v94.2.1)

## Nimbus ⛅️🔬🔭

### What's Changed
 - Added `applyLocalExperiments()` method as short hand for `setLocalExperiments` and `applyPendingExperiments`. ([#5131](https://github.com/mozilla/application-services/pull/5131))
   - `applyLocalExperiments` and `applyPendingExperiments` now returns a cancellable job which can be used in a timeout.
   - `initialize` function takes a raw resource file id, and returns a cancellable `Job`.

### What's Fixed

   - A regression affecting Android in calculating `days_since_install` and `days_since_update` ([#5157](https://github.com/mozilla/application-services/pull/5157))

# v94.2.0 (_2022-09-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.1.0...v94.2.0)

# General
  - `error-support` is now exposed to both Firefox iOS and Focus iOS. `error-support` supports better error reporting and logging for errors. ([#5094](https://github.com/mozilla/application-services/pull/5094))

## Nimbus FML ⛅️🔬🔭🔧
### What's Changed
  - Add `channels` value for defaults and add support for multiple channels in `channel` via comma separation. ([#5101](https://github.com/mozilla/application-services/pull/5101))

### ✨ What's New ✨
  - JEXL targeting allows for using the `in` keyword with objects and a map of active experiments has been added to TargetingAttributes. The map will always be empty at this time. ([#5104](https://github.com/mozilla/application-services/pull/5104))

## Nimbus ⛅️🔬🔭

### What's Changed
 - Added metrics for SDK unavailability and disk cache unreadiness to tease apart the difference in startup slowness. ([#5118](https://github.com/mozilla/application-services/pull/5118))

# v94.1.0 (_2022-08-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.0.1...v94.1.0)
## Nimbus
### What's new
- Added telemetry to track how often apps query for variables before Nimbus is initialized. ([#5091](https://github.com/mozilla/application-services/pull/5091))


# v94.0.1 (_2022-08-09_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v94.0.0...v94.0.1)

## Nimbus FML
### What's fixed
  - Linux releases for the FML were missing, there are available again now. ([#5080](https://github.com/mozilla/application-services/pull/5080))

# v94.0.0 (_2022-08-02_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.8.0...v94.0.0)

## Logins
### ⚠️ Breaking Changes ⚠️
  - Removed expired logins sqlcipher migration metrics and renamed the `migrateLoginsWithMetrics` function since it no longer reports metrics. An associated iOS PR ([#11470](https://github.com/mozilla-mobile/firefox-ios/pull/11470)) has been created to address the function renaming. ([#5064](https://github.com/mozilla/application-services/pull/5064))

# v93.8.0 (_2022-07-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.7.1...v93.8.0)

## Nimbus FML ⛅️🔬🔭
### What's Changed
  - Validate the configuration passed from a top-level FML file to imported files. ([#5055](https://github.com/mozilla/application-services/pull/5055))

## Places
### What's new
  - We now expose all of the Places history APIs to Swift consumers. ([#4989](https://github.com/mozilla/application-services/pull/4989))
  - Added an optional db_size_limit parameter to `run_maintenance`.  This can be used to set a target size for the places DB.  If the DB is over that size, we'll prune a few older visits. The number of visits is very small (6) to ensure that the operation only blocks the database for a short time. The intention is that `run_maintenance()` is called frequently compared to how often visits are added to the places DB.

## Sync15
### What's changed
  - `CLIENTS_TTL` has been updated to be 180 days instead of 21 ([#5054](https://github.com/mozilla/application-services/pull/5054))

# v93.7.1 (_2022-07-26_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.7.0...v93.7.1)

## Places
### What's changed
  - The `delete_visits_between` API now also deletes history metadata ([#5046](https://github.com/mozilla/application-services/pull/5046))

# v93.7.0 (_2022-07-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.6.0...v93.7.0)

## Nimbus FML ⛅️🔬🔭🔧
### What's Changed
  - Added `MOZ_APPSERVICES_MODULE` environment variable to specify the megazord module for iOS ([#5042](https://github.com/mozilla/application-services/pull/5042)). If it is missing, no module is imported.
### ✨ What's New ✨
  - Enabled remote loading and using configuring of branches. ([#5041](https://github.com/mozilla/application-services/pull/5041))
  - Add a `fetch` command to `nimbus-fml` to demo and test remote loading and paths. ([#5047](https://github.com/mozilla/application-services/pull/5047))

## Logins
### What's Changed
  - Updated the `LoginsStorageError` implementation and introduce error reporting for unexpected errors.
    Note that some errors were removed, which is technically a breaking change, but none of our
    consumers use those errors so it's not a breaking change in practice.

# v93.6.0 (_2022-07-11_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.5.0...v93.6.0)

## Autofill

### What's Fixed
  - Fixed syncing of autofill when tombstones exist in the local mirror (#5030)

## Nimbus FML ⛅️🔬🔭🔧

### What's New
  - Added support for breaking up FML files using `includes` and separating into different modules with `imports`.
    ([#5031](https://github.com/mozilla/application-services/pull/5031), [#5022](https://github.com/mozilla/application-services/pull/5022), [#5016](https://github.com/mozilla/application-services/pull/5016), [#5014](https://github.com/mozilla/application-services/pull/5014), [#5007](https://github.com/mozilla/application-services/pull/5007), [#4999](https://github.com/mozilla/application-services/pull/4999), [#4997](https://github.com/mozilla/application-services/pull/4997), [#4976](https://github.com/mozilla/application-services/pull/4976))
    - This is _not_ a breaking change, but should be accompanied by a upgrade to the megazord ([#4099](https://github.com/mozilla/application-services/pull/4099)).
    - This also deprecates some commands in the command line interface ([#5022](https://github.com/mozilla/application-services/pull/5022)). These will be removed in a future release.
    - Related proposal document: [FML: Imports and Includes](https://experimenter.info/fml-imports-and-includes).

## Logins

### What's Changed
  - sqlcipher migrations no longer record metrics (#5017)

## Glean
### What's Changed
  - Updated to Glean v50.1.2

## UniFFI
### What's Changed
  - Updated to UniFFI 0.19.3

# v93.5.0 (_2022-06-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.4.0...v93.5.0)

## Nimbus FML ⛅️🔬🔭🔧

### What's Changed
  - Added [`includes` list property](https://experimenter.info/fml-imports-and-includes/#the-include-list) to enable splitting up large `nimbus.fml.yaml` files ([#4976](https://github.com/mozilla/application-services/pull/4976)).

# v93.4.0 (_2022-06-09_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.3.0...v93.4.0)

## General

### What's Changed

* Glean updated to v50 and all internal calls adopted

# v93.3.0 (_2022-06-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.2.2...v93.3.0)

## Error-support
### What's New
  - Added a new error reporting system that is intended to eventually replace using `log::error` to report errors
  - Added code using the new system to track down application-services#4856
  - Added UniFFI API for this crate.  Consumers should use this to register for error reports and breadcrumbs.

# v93.2.2 (_2022-05-27_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.2.1...v93.2.2)

## Tabs
### What's Changed

- Fixed the iOS breaking change in the `SyncUnlockInfo` constructor by making `tabsLocalId` an optional parameter ([#4975](https://github.com/mozilla/application-services/pull/4975)).

# v93.2.1 (_2022-05-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.2.0...v93.2.1)

## General
### What's new
- Uniffi was upgraded to 0.18.0. For our consumers, this means there now exported types that used to be internal to `uniffi`. ([#4949](https://github.com/mozilla/application-services/pull/4949)).
  - The types are:
    - `Url` alias for `string`
    - `PlacesTimestamp` alias for`i64`
    - `VisitTransitionSet` alias for `i32`
    - `Guid` alias for `string`
    - `JsonObject` alias for `string`
  - Non of the exposed types conflict with a type in iOS so this is not a breaking change.

## Nimbus ⛅️🔬🔭

### What's new

- Make generation of Experimenter compatible YAML repeatable: fields, variables, features and enum variants are listed alphabetically. ([#4964](https://github.com/mozilla/application-services/pull/4964)).

## Tabs
### What's Changed

- The component has been updated for integration into Firefox iOS ([#4905](https://github.com/mozilla/application-services/pull/4905)).
  - The `DeviceType` naming conflict which prevented `rust-components-swift` from generating Tabs code has been resolved.
  - Errors and the `reset` function have been exposed.
  - Parameters for the `sync` function have been updated to match the `SyncUnlockInfo` parameters.
  - The `tabs-sync` example has been updated with the above changes.

# v93.2.0 (_2022-05-11_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.1.0...v93.2.0)

## General
### What's new
- Application services now releases a **separate** xcframework with only the components needed by focus-ios (namely Nimbus, Viaduct and Rustlog). This change is only relevant for focus, it does not affect the already existing xcframework for firefox ios. ([#4953](https://github.com/mozilla/application-services/pull/4953))

# v93.1.0 (_2022-05-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.0.4...v93.1.0)

## Nimbus ⛅️🔬🔭

### What's New
  - New API in the `FeatureHolder`, both iOS and Android to control the output of the `value()` call:
    - to cache the values given to callers; this can be cleared with `FxNimbus.invalidatedCachedValues()`
    - to add a custom initializer with `with(initializer:_)`/`withInitializer(_)`.
## Places
### What's Fixed:
- Fixed a bug in Android where non-fatal errors were crashing. ([#4941](https://github.com/mozilla/application-services/pull/4941))
- Fixed a bug where querying history metadata would return a sql error instead of the result ([4940](https://github.com/mozilla/application-services/pull/4940))
### What's new:
- Exposed the `deleteVisitsFor` function in iOS, the function can be used to delete history metadata. ([#4946](https://github.com/mozilla/application-services/pull/4946))
  - Note: The API is meant to delete all history, however, iOS does **not** use the `places` Rust component for regular history yet.

# v93.0.4 (_2022-04-28_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.0.3...v93.0.4)

## Places
### What's New
- The `delete_visits_for()` function now deletes all history metadata even when the item is
  bookmarked.

## Nimbus
### What's fixed
- Fixed a bug where the visibility of `GetSdk` was internal and it was used in generated FML code. ([#4927](https://github.com/mozilla/application-services/pull/4927))

# v93.0.3 (_2022-04-27_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.0.2...v93.0.3)

## Nimbus ⛅️🔭🔬

### What's New
  - Added targeting attributes for `language` and `region`, based upon the `locale`. [#4919](https://github.com/mozilla/application-services/pull/4919)
    - This also comes with an update in the JEXL evaluator to handle cases where `region` is not available.

### What's Changed
  - Fixed: A crash was detected by the iOS team, which was traced to `FeatureHolder.swift`. ([#4924](https://github.com/mozilla/application-services/pull/4924))
    - Regression tests added, and FeatureHolder made stateless in both Swift and Kotlin.

# v93.0.2 (_2022-04-25_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.0.1...v93.0.2)

## Nimbus FML
### What's fixed
- (iOS only) Made the extensions on `String` and `Variables` public. The extended functions are used in the generated code and that didn't compile in consumers when internal.

# v93.0.1 (_2022-04-20_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v93.0.0...v93.0.1)

## Nimbus FML ⛅️🔬🔭🔧

### What's Fixed
  - Handling of optional types which require a mapping to a usable type. ([#4915](https://github.com/mozilla/application-services/pull/4915))

## Places

- Downgraded places `get_registered_sync_engine` `log:error` to `log:warn` to fix an issue where places was unnecessarily creating sentry noise. This change was also cherry-picked to [v91.1.2](#v9112-2022-04-19)

# v93.0.0 (_2022-04-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v92.0.1...v93.0.0)

## Nimbus ⛅️🔭🔬 + Nimbus FML ⛅️🔬🔭🔧

### What's New

- Add support for bundled resources in the FML in Swift. This corresponds to the `Image` and `Text` types. [#4892](https://github.com/mozilla/application-services/pull/4892)
  - This must include an update to the megazord, as well re-downloading the `nimbus-fml` binary.
  - Kotlin support for the same has also changed to match the Swift implementation, which has increased performance.

# v92.0.1 (_2022-03-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v92.0.0...v92.0.1)

## Nimbus FML ⛅️🔬🔭🔧

### What's Fixed

- Swift: a bug in our understanding of Swift optional chaining rules meant that maps with a mapping and merging produced invalid code. ([#4885](https://github.com/mozilla/application-services/pull/4885))

## General

### What's Changed

- Added documentation of our sqlite pragma usage. ([#4876](https://github.com/mozilla/application-services/pull/4876))

# v92.0.0 (_2022-03-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.1.0...v92.0.0)

## Places
### ⚠️ Breaking Changes ⚠️
- Removed some functions related to sync interruption.  These were never really completed and don't seem to be in use by iOS/Android code:
  - `PlacesApi.new_sync_conn_interrupt_handle()`
  - Swift only: `PlacesAPI.interrupt()`
- The exception variant `InternalPanic` was removed. It's only use was replaced by the already existing `UnexpectedPlacesException`. ([#4847](https://github.com/mozilla/application-services/pull/4847))
### What's New
- The Places component will report more error variants to telemetry. ([#4847](https://github.com/mozilla/application-services/pull/4847))
## Autofill / Logins / Places / Sync Manager, Webext-Storage
### What's Changed
- Updated interruption handling and added support for shutdown-mode which interrupts all operations.

## Tabs
### ⚠️ Breaking Changes ⚠️

- The tabs component's constructor now requires the path to the database file where remote tabs will be persisted to.
- Requesting remote tabs before the first sync will now return the tabs in this database, so may be "stale".
## Glean
### ⚠️ Breaking Changes ⚠️
### Swift
- GleanMetrics should now be imported under `import Glean` instead of importing via `MozillaRustComponents`

## Nimbus FML
### What's Changed
- Papercut fixes for nicer developer experience [#4867](https://github.com/mozilla/application-services/pull/4867)
  - More helpful validation error reporting
  - Better handling of defaults in objects and enum maps
  - More YAML syntactic checking.
- Allow experimenter to output to a YAML file, as well as JSON. [#4874](https://github.com/mozilla/application-services/pull/4874)
  - If the file extension is `yaml`, then output as YAML, otherwise, output as JSON.
# v91.1.2 (_2022-04-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.1.1...v91.1.2)

**IMPORTANT**: The following change was cherry-picked to 91.1.2 which was a release **not** from the main branch. The change then landed in [v93.0.1](#v9301-2022-04-20). This means that versions v92.0.0 - v93.0.0 do not have the change.
## Places

- Downgraded places `get_registered_sync_engine` `log:error` to `log:warn` to fix an issue where places was unnecessarily creating sentry noise.

# v91.1.1 (_2022-03-23_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.1.0...v91.1.1)

## Autofill
### What's New
  - Added `temp-store`, `journal-mode`, and `foreign-keys` pragmas to autofill component. ([#4882](https://github.com/mozilla/application-services/pull/4882))

# v91.1.0 (_2022-02-11_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.0.1...v91.1.0)

## ⛅️🔬🔭 Nimbus SDK

### What's fixed

- Fixes a bug where disabling studies did not disable rollouts. ([#4807](https://github.com/mozilla/application-services/pull/4807))

### ✨ What's New ✨

- A message helper is now available to apps wanting to build a Messaging System on both Android and iOS. Both of these access the variables
  provided by Nimbus, and can have app-specific variables added. This provides two functions:
  - JEXL evaluation ([#4813](https://github.com/mozilla/application-services/pull/4813)) which evaluates boolean expressions.
  - String interpolation ([#4831](https://github.com/mozilla/application-services/pull/4831)) which builds strings with templates at runtime.

## Xcode

- Bumped Xcode version from 13.1.0 -> 13.2.1

## Nimbus FML
### What's fixed
- Fixes a bug where each time the fml is run, the ordering of features in the experimenter json is changed. ([#4819](https://github.com/mozilla/application-services/pull/4819))

# v91.0.1 (_2022-02-02_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v91.0.0...v91.0.1)

## Places

### What's Changed
  - The database initialization code now uses BEGIN IMMEDIATE to start a
    transaction.  This will hopefully prevent `database is locked` errors when
    opening a sync connection.

### What's New

  - The `HistoryVisitInfo` struct now has an `is_remote` boolean which indicates whether the
    represented visit happened locally or remotely. ([#4810](https://github.com/mozilla/application-services/pull/4810))

# v91.0.0 (_2022-01-31_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v90.0.1...v91.0.0)

## Nimbus FML

### What's New
  - The Nimbus FML can now generate swift code for the feature manifest. ([#4780](https://github.com/mozilla/application-services/pull/4780))
    - It can be invoked using:
    ```sh
    $ nimbus-fml <FEATURE_MANIFEST_YAML> -o <OUTPUT_NAME> ios features
    ```
    - You can check the support flags and options by running:
    ```sh
    $ nimbus-fml ios --help
    ```
    - The generated code exposes:
      -  a high level nimbus object, whose name is configurable using the `--classname` option. By default the object is `MyNimbus`.
      - All the enums and objects defined in the manifest as idiomatic Swift code.
    - Usage:
      - To access a feature's value:
        ```swift
        // MyNimbus is the class that holds all the features supported by Nimbus
        // MyNimbus has an singleton instance, you can access it using the `shared` field:

        let nimbus = MyNimbus.shared

        // Then you can access the features using:
        // MyNimbus.features.<featureNameCamelCase>.value(), for example:

        let feature = nimbus.features.homepage.value()
        ```
      - To access a field in the feature:
        ```swift
        // feature.<propertyNameCamelCase>, for example:

        assert(feature.sectionsEnabled[HomeScreenSection.topSites] == true)
        ```

### ⚠️ Breaking Changes ⚠️

  - **Android only**: Accessing drawables has changed to give access to the resource identifier. ([#4801](https://github.com/mozilla/application-services/pull/4801))
    - Migration path to the old behaviour is:

    ```kotlin
    let drawable: Drawable = MyNimbus.features.exampleFeature.demoDrawable
    ```

    becomes:
    ```kotlin
    let drawable: Drawable = MyNimbus.features.exampleFeature.demoDrawable.resource
    ```
## General iOS
### What's changed
- Moved `SwiftKeychainWrapper` from an external Swift Package to be bundled with FxA. This is due to issues Firefox iOS had with their dependency tree. ([#4797](https://github.com/mozilla/application-services/pull/4797))
- Exposed all crates as targets for the XCFramework. ([#4797](https://github.com/mozilla/application-services/pull/4797))

# v90.0.1 (_2022-01-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v90.0.0...v90.0.1)

## Places
  - Fixed an issue with previously consumed errors for invalid URLs were propagating to consumers and causing a crash
    - Changed `bookmarks_get_all_with_url` and `accept_result` to accept a string instead of url


# v90.0.0 (_2022-01-20_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v89.0.0...v90.0.0)

## Places

### ⚠️ Breaking Changes ⚠️
  - Places has been completely UniFFI-ed

# v89.0.0 (_2022-01-20_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v88.0.0...v89.0.0)

## Supported Xcode Versions
- Reverting the supported Xcode version from 13.2.1 to 13.1.0 to circumvent the issues with Swift Package Manager in Xcode 13.2.1. ([#4787](https://github.com/mozilla/application-services/pull/4787))
## Nimbus☁️🔬🔭

### What's New
   - Add `Text` and `Image` support for the FML to access bundled resources ([#4784](https://github.com/mozilla/application-services/pull/4784)).

### Breaking Change
  - The `NimbusInterface` now exposes a `context: Context` property.

# v88.0.0 (_2022-01-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v87.3.0...v88.0.0)

## Nimbus☁️🔬🔭

### What's Changed
  - The SDK is now tolerant to legacy experiment recipes that have both `feature` and `features` in their branches ([SDK-1989](https://github.com/mozilla/application-services/pull/4777))

## General

### ⚠️ Breaking Changes ⚠️

  - The bundled version of Glean has been updated to v43.0.2.
    See [the Glean Changelog](https://github.com/mozilla/glean/blob/v43.0.2/CHANGELOG.md) for full details.
    BREAKING CHANGE: Pass build info into initialize, which contains the build date.
    A suitable instance is generated by `glean_parser` in `GleanMetrics.GleanBuild.info`.

# v87.3.0 (_2022-01-11_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v87.2.0...v87.3.0)

## Supported Xcode Versions
- As of Jan 2022, support for Xcode version 13.2.1 is upcoming. After the associated PR is merged AS side and a release is cut, Fx-iOS will update on their side to fully support this Xcode version. See Fx-iOS's Wiki for details.

## viaduct
### What's New
- Add support for PATCH methods. ([#4751](https://github.com/mozilla/application-services/pull/4751))

## Nimbus
### What's new
  - The Nimbus SDK now support application version targeting, where experiment creators can set `app_version|versionCompare({VERSION}) >= 0` and the experiments will only target users running `VERSION` or higher. ([#4752](https://github.com/mozilla/application-services/pull/4752))
      - The `versionCompare` transform will return a positive number if `app_version` is greater than
      `VERSION`, a negative number if `app_version` is less than `VERSION` and zero if they are equal
      - `VERSION` must be passed in as a string, for example: `app_version|versionCompare('95.!') >= 0` will target users who are on any version starting with `95` or above (`95.0`, `95.1`, `95.2.3-beta`, `96` etc..)

# v87.2.0 (_2021-12-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v87.1.0...v87.2.0)

### ✨✨ What's New ✨✨

#### ⛅️🔭🔬 Nimbus

- Initial release of the Nimbus Feature Manifest Language tool (`nimbus-fml`).
  - This is a significant upgrade to the Variables API, adding code-generation to Kotlin and Experimenter compatible manifest JSON.
  - [RFC for language specification](https://github.com/mozilla/experimenter-docs/pull/156).
  - This is the first release it is made available to client app's build processes.
  - [Build on CI](https://github.com/mozilla/application-services/pull/4701) ready for application build processes to download.

# v87.1.0 (_2021-12-02_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v87.0.0...v87.1.0)

## Logins
### What's changed
  - The `update()` and `add_or_update()` methods will log rather than return an error when trying to update a duplicate login (#4648)

## Logins, Places, SyncManager
### What's Changed
  - These packages all use `parking_lot::Mutex` instead of `std::Mutex`, meaning we should no
    longer see errors about mutexes being poisoned.

## Push
### What's fixed
  - Fixes a bug where the subscriptions would fail because the server didn't return the `uaid`, this seems to happen only when the client sends request that include the `uaid`.([#4697](https://github.com/mozilla/application-services/pull/4697))

# General

- We now use xcode 13.1 to generate our iOS build artifacts. ([#4692](https://github.com/mozilla/application-services/pull/4692))

# v87.0.0 (_2021-11-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v86.2.0...v87.0.0)

## Push
### What's changed
  - Push internally no longer uses the `error_support` dependency to simplify the code. It now directly defines exactly one error enum and exposes that to `uniffi`. This should have no implication to the consumer code ([#4650](https://github.com/mozilla/application-services/pull/4650))

## Places
### ⚠️ Breaking Changes ⚠️
  - Switched sync manager integration to use `registerWithSyncManager()` like the other components ([#4627](https://github.com/mozilla/application-services/pull/4627))

## SyncManager

### ⚠️ Breaking Changes ⚠️
  - Updated SyncManager to use UniFFI:
    - SyncManager is now a class that gets instatiated rather than a singleton
    - Added more SyncManagerException subclasses
    - SyncParams.engines is now a SyncEngineSelection enum.
      SyncEngineSelection's variants are All, or Some(engine_list).  This
      replaces the old code which used null to signify all engines.
    - SyncResult.telemetry was replaced with SyncResult.telemetryJson.
    - There were a handful of naming changes:
      - SyncAuthInfo.tokenserverURL -> SyncAuthInfo.tokenserverUrl
      - DeviceSettings.type -> DeviceSettings.kind

# v86.2.0 (_2021-11-02_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v86.1.0...v86.2.0)

## Push
### What's Changed
  - We've changed the database schema to avoid confusion about the state of subscriptions and
    in particular, avoid `SQL: UNIQUE constraint failed: push_record.channel_id` errors
    reported in [#4575](https://github.com/mozilla/application-services/issues/4575). This is
    technically a breaking change as a dictionary described in the UDL changed, but in practice,
    none of our consumers used it, so we are not declaring it as breaking in this context.

## Logins

### What's New

  - Added support for recording telemetry when the logins encryption key needs to be regenerated.

# v86.1.0 (_2021-10-27_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v86.0.0...v86.1.0)

## ⛅️🔬🔭 Nimbus

### What's New

  - Rollouts: allows winning branch promotion and targeting rollouts of features. [#4567](https://github.com/mozilla/application-services/pull/4567).
    - for both Android and iOS.

### What's fixed
  - Fixed a bug in iOS where the installation date would be set to start of EPOCH ([#4597](https://github.com/mozilla/application-services/pull/4597))
  - Fixed a bug in Android where we were missing disqualification events after a global opt-out ([#4606](https://github.com/mozilla/application-services/pull/4606))

## Push

  - We've changed how the push database is opened, which should mean we now automatically handle
    some kinds of database corruption.

## General
### What's changed
  - The bundled version of Glean has been updated to v42.0.1.
    See [the Glean Changelog](https://github.com/mozilla/glean/blob/v42.0.1/CHANGELOG.md) for full details.
    (Note there is a breaking change in Rust, but that doesn't impact consumers of Application Services)

# v86.0.1 (_2021-10-28_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v86.0.0...v86.0.1)
## Logins
### What's Changed
- Downgraded the log level of some logs, so now they should not show up in Sentry.


# v86.0.0 (_2021-10-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.4.1...v86.0.0)

## Logins

### ⚠️ Breaking Changes ⚠️
  - Rework logins to no longer use sqlcipher and instead use plain sqlite. This is a major change
    with a massive impact on all consumers of this component, all of whom are already aware of
    this change and have PRs in-progress.
    ([#4549](https://github.com/mozilla/application-services/pull/4549))

# v85.4.2 (_2021-10-20_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.4.1...v85.4.2)

## Nimbus
### What's fixed
- Fixed a bug in iOS where the installation date would be set to start of EPOCH ([#4597](https://github.com/mozilla/application-services/pull/4597))


# v85.4.1 (_2021-10-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.4.0...v85.4.1)

## Logins

- Metrics for the logins sqlcipher migration are included to help "bootstrap"
  the metrics ready for the migration.

# v85.4.0 (_2021-10-05_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.3.0...v85.4.0)

## Sync

### What's Changed

- Clients engine now checks for tombstones and any deserialisation errors when receiving a client record, and ignores
  it if either are present ([#4504](https://github.com/mozilla/application-services/pull/4504))

## Nimbus
### What's changed
- The DTO changed to remove the `probeSets` and `enabled` fields that were previously unused. ([#4482](https://github.com/mozilla/application-services/pull/4482))
- Nimbus will retry enrollment if it previously errored out on a previous enrollment.

# v85.3.0 (_2021-09-30_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.2.0...v85.3.0)

## Nimbus

### What's new

- Nimbus can now target on `is_already_enrolled`. Which is true only if the user is already enrolled in experiment. ([#4490](https://github.com/mozilla/application-services/pull/4490))
- Nimbus can now target on `days_since_install` and `days_since_update`. Which reflect the days since the user installed the application and the days since the user last updated the application. ([#4491](https://github.com/mozilla/application-services/pull/4491))
- Android only: the observer method `onExperimentsApplied()` is now called every time `applyPendingExperiments()` is called. This is to bring it in line with iOS.

# v85.2.0 (_2021-09-28_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.1.0...v85.2.0)

## Places
### What's New
  - Added Swift bindings for the following History Metadata APIs: `getHighlights` and `deleteHistoryMetadata`.

# v85.1.0 (_2021-09-27_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v85.0.0...v85.1.0)

## General

### What's Changed

- Rust toolchain has been bumped to 1.55 and minimum version bumped to 1.53 to comply with our [Rust Policy](https://github.com/mozilla/application-services/blob/main/docs/rust-versions.md#application-services-rust-version-policy)

- Xcode has been updated to version 13
  - application-services noq uses the new build system by default

## Nimbus

### What's Changed

- 🐞🍏 Bugfix, iOS only — Increased visibility for `Dictionary` extensions when working with `FeatureVariables` and `enums`.


# v85.0.0 (_2021-09-22_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v84.0.0...v85.0.0)

## Places, Autofill, Webext-Storage

### What's Changed

- Databases which are detected as being corrupt as they are opened will be deleted and re-created.

## Nimbus

### What's New

- [#4455][1]: For both iOS and Android: extra methods on `Variables` to support orderable items:
  - `getEnum` to coerce strings into Enums.
  - `get*List`, `get*Map` to get lists and maps of all types.
  - Dictionary/Map extensions to map string keys to enum keys, and string values to enum values.
- Nimbus now supports multiple features on each branch. This was added with backward compatibility to ensure support for both schemas. ([#4452](https://github.com/mozilla/application-services/pull/4452))
### ⚠️ Breaking Changes ⚠️

- [#4455][1]: Android only: method `Variables.getVariables(key, transform)`, `transform` changes type
  from `(Variables) -> T` to `(Variables) -> T?`.

[1]: https://github.com/mozilla/application-services/pull/4455

# v84.0.0 (_2021-09-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v83.0.0...v84.0.0)

## Places

### ⚠️ Breaking Changes ⚠️
  - `previewImageUrl` property was added to `HistoryMetadata` ([#4448](https://github.com/mozilla/application-services/pull/4448))
### What's changed
  - `previewImageUrl` was added to `VisitObservation`, allowing clients to make observations about the 'hero' image of the webpage ([#4448](https://github.com/mozilla/application-services/pull/4448))

## Push
### ⚠️ Breaking Changes ⚠️
  - The push component now uses `uniffi`! Here are the Kotlin breaking changes related to that:
     - `PushAPI` no longer exists, consumers should consumer `PushManager` directly
     - `PushError` becomes `PushException`, and all specific errors are now `PushException` children, and can be retrieved using `PushException.{ExceptionName}`, for example `StorageError` becomes `PushException.StorageException`
     - The `PushManager.decrypt` function now returns a `List<Byte>`, where it used to return `ByteArray`, the consumer can do the conversion using `.toByteArray()`
     - All references to `channelID` become `channelId` (with a lowercase `d`)

# v83.0.0 (_2021-09-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v82.3.0...v83.0.0)

## Android

### ⚠️ Breaking Changes ⚠️
  - Many error classes have been renamed from `FooError` or `FooErrorException` to just `FooException`,
    to be more in keeping with Java/Kotlin idioms.
    - This is due to UniFFi now replacing trailing 'Error' named classes to 'Exception'

## Autofill

### ⚠️ Breaking Changes ⚠️
  - The `Error` enum is now called `AutofillError` (`AutofillException` in Kotlin) to avoid conflicts with builtin names.

## Push
### ⚠️ Breaking Changes ⚠️
- The `unsubscribeAll` API will now return nothing as opposed to a boolean. The boolean was misleading as it only ever returned true.
  errors can be caught using exceptions. ([#4418](https://github.com/mozilla/application-services/pull/4418))
### What's Changed
 - The push `unsubscribe` API will no longer accept a null `channel_id` value, a valid `channel_id` must be presented, otherwise rust will panic and an error will be thrown to android.
  note that this is not a breaking change, since our hand-written Kotlin code already ensures that the function can only be called with a valid, non-empty, non-nullable string. ([#4402](https://github.com/mozilla/application-services/pull/4402))

## Nimbus

### What's Changed

- `DatabaseNotReady` exceptions are no longer reported to the error reporter on either Android or iOS. [#4438](https://github.com/mozilla/application-services/pull/4438)
- `NimbusErrorException` has been renamed `NimbusException`. This internal API, so is not a breaking change.

# v82.3.0 (_2021-08-30_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v82.2.0...v82.3.0)

### What's New
  - Changed how shared libraries are loaded to avoid an issue when both uniffi
    and `Helpers.kt` wants to load the same library ([#4412](https://github.com/mozilla/application-services/pull/4412))


# v82.2.0 (_2021-08-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v82.1.0...v82.2.0)

## Push
### What's changed
  - The push component will now attempt to auto-recover from the server losing its UAID ([#4347](https://github.com/mozilla/application-services/pull/4347))
    - The push component will return a new kotlin Error `UAIDNotRecognizedError` in cases where auto-recovering isn't possible (when subscribing)
    - Two other new errors were defined that were used to be reported under a generic error:
      - `JSONDeserializeError` for errors in deserialization
      - `RequestError` for errors in sending a network request

## Nimbus
### What's changed
   - Nimbus on iOS will now post a notification when it's done fetching experiments, to match what it does when applying experiments. ([#4378](https://github.com/mozilla/application-services/pull/4378))

# v82.1.0 (_2021-07-30_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v82.0.0...v82.1.0)

## Places

### What's New

- The Swift bindings for history metadata enums and structs now have
  public initializers, allowing them to be used properly from Swift.
  ([#4371](https://github.com/mozilla/application-services/pull/4371))


# v82.0.0 (_2021-07-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v81.0.1...v82.0.0)

## General

### ⚠️ Breaking Changes ⚠️

  - The bundled version of Glean has been updated to v39.0.4, which includes a new API
    for recording extra event fields that have an explicit type..
    ([#4356](https://github.com/mozilla/application-services/pull/4356))

### What's New

  - Added content signature and chain of trust verification features in `rc_crypto`,
    and updated NSS to version 3.66.
    ([#4195](https://github.com/mozilla/application-services/pull/4195))

## Nimbus

### What's Changed

  - The Nimbus API now accepts application specific context as a part of its `appSettings`.
    The consumers get to define this context for targeting purposes. This allows different consumers
    to target on different fields without the SDK having to acknowledge all the fields.
    ([#4359](https://github.com/mozilla/application-services/pull/4359))

# v81.0.1 (_2021-07-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v81.0.0...v81.0.1)

## Tabs
### ⚠️ Breaking Changes ⚠️
Note: Though this is technically a breaking change, we do not expect any consumers to have upgraded to v81. Since this was a bug that was introduced in v81 we're treating it as a bugfix.

    - Tab struct member last_used is now a i64

# v81.0.0 (_2021-07-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v80.0.1...v81.0.0)

## Tabs
### ⚠️ Breaking Changes ⚠️
 - Tabs has been Uniffi-ed! ([#4192](https://github.com/mozilla/application-services/pull/4192))
    - Manual calling of sync() is removed
    - registerWithSyncManager() should be used instead
    - Tab struct member lastUsed is now a U64
## Nimbus
### What's changed
  - Fixed a bug where opt-in enrollments in experiments were not preserved when the application is restarted ([#4324](https://github.com/mozilla/application-services/pull/4324))
  - The nimbus component now specifies the version of the server's api - currently V1. That was done to avoid redirects. ([#4319](https://github.com/mozilla/application-services/pull/4319))

## Push
### What's changed
  - Fixed a bug where we don't delete a client's UAID locally when it's deleted on the server ([#4325](https://github.com/mozilla/application-services/pull/4325))


# v80.0.1 (_2021-07-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v80.0.0...v80.0.1)

## General

### What's Changed

- Updated CircleCI xcode version to 12.5.1

[Full Changelog](https://github.com/mozilla/application-services/compare/v80.0.0...main)

# v80.0.0 (_2021-06-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v79.0.2...v80.0.0)

## fxa-client
### ⚠️ Breaking Changes ⚠️
  - The old StateV1 persisted state schema is now removed. ([#4218](https://github.com/mozilla/application-services/pull/4218))
    Users on very old versions of this component will no longer be able to cleanly update to this version. Instead, the consumer code
    will receive an error indicating that the schema was not correctly formatted.

## Nimbus
### What's Changed
  - Nimbus SDK now supports different branches having different Feature Configs ([#4213](https://github.com/mozilla/application-services/pull/4213))

## Other
  - `./libs/build-all.sh` now displays a more helpful error message when a file fails checksum integrity test.

# v79.0.2 (_2021-06-23_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v79.0.1...v79.0.2)

- Removed hard crash for `migrateToPlaintextHeader` and allowed error to propagate for Logins

# v79.0.1 (_2021-06-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v79.0.0...v79.0.1)

## Logins

- Fixed a bug on iOS where `getDbSaltForKey` would incorrectly trigger a fatal error
  instead of propagating errors to the caller.

## Dependencies

- The version of UniFFI used to generate Rust component bindings was updated to v0.12.0.

# v79.0.0 (_2021-06-09_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v78.0.0...v79.0.0)

## Logins

### ⚠️ Breaking changes ⚠️

Logins now Uniffi-ed! While this is a very large change internally, the externally visible changes are:

- The name and types of exceptions have changed - the base class for errors is LoginsStorageErrorException.
- The struct `ServerPassword` (Android) and `LoginRecord` (iOS) is now named `Login` with the formSubmitURL field now formSubmitUrl

## [viaduct-reqwest]

### What's Changed

- Update viaduct-reqwest to use reqwest 0.11. ([#4146](https://github.com/mozilla/application-services/pull/4146))

# v78.0.0 (_2021-06-01_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v77.0.2...v78.0.0)

## [places]

### ⚠️ Breaking Changes ⚠️
  - History Metadata API shape changed to follow an observation pattern, similar to what is present for History.
    Shape of objects and related DB schema changed as well. See ([#4123](https://github.com/mozilla/application-services/pull/4123))

# v77.0.2 (_2021-05-31_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v77.0.1...v77.0.2)

## Autofill

 - Fixed a failing assertion when handling local dupes during a sync (#4154)

# v77.0.1 (_2021-05-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v77.0.0...v77.0.1)

## CI-only to force iOS artifact build, 77.0.0 is not good for iOS
  - add --force flag when installing swift-protobuf via homebrew (#4137)

# v77.0.0 (_2021-05-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v76.0.1...v77.0.0)

## Logins

 - Split the DB from the sync engine (#4129)
 - Rename LoginStore to LoginsSyncEngine (#4124)

## Nimbus ☁️🔬

### What's New

 - Both Android and iOS gain a `nimbus.getVariables(featureId: String)` and a new wrapper around JSON data coming straight from Remote Settings.
 - Application features can only have a maximum of one experiment running at a time.
 - Enable consuming applications to change the server collection being used. (#4076)

### What's Changed
 - Add manual feature exposure recording (#4120)
 - Android and iOS `Branch` objects no longer have access to a `FeatureConfig` object.
 - Localized strings and images are provided by the app, but usable from nimbus. (#4133)

### ⚠️ Breaking changes ⚠️
 - Migrate the experiment database from version 1 to version 2 on first run .(#4078)
   - Various kinds of incorrectly specified feature and featureId
   related fields will be detected, and any related experiments & enrollments
   will be discarded.
   - Experiments & enrollments will also be discarded if they
   are missing other required fields (eg schemaVersion).
   - If there is an error
   during the database upgrade, the database will be wiped, since losing
   existing enrollments is still less bad than having the database in an unknown
   inconsistent state.

# v76.0.1 (_2021-05-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v76.0.0...v76.0.1)

## Autofill

Fixed an error migrating from version 1 to version 2 of the database.

# v76.0.0 (_2021-05-12_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.2.0...v76.0.0)

## History Metadata Storage

- Introduced a new experimental metadata storage API, part of libplaces.

## Sync Manager

- Removed support for the wipeAll command (#4006)

## Autofill

### What's Changed

- Added support to scrub encrypted data to handle lost/corrupted client keys.
  Scrubbed data will be replaced with remote data on the next sync.

## Nimbus

 - Added bucket and collections to `NimbusServerSettings`, with default values.
 - Added `getAvailableExperiments()` method exposed by `NimbusClient`.
 - At most one local experiment will be enrolled for any given `featureId`, and
  to support this, the database can now have a NotEnrolledReason::FeatureConflict value.

### ⚠️ Breaking changes ⚠️

- Moved the `Nimbus` class and its test class from Android Components into this repository. Existing integrations should pass a delegate in to provide Nimbus with a thread to do I/O and networking on, and an Observer.
  Fixed in the complementary [android-components#10144](https://github.com/mozilla-mobile/android-components/pull/10144)


# v75.2.0 (_2021-04-20_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.1.0...v75.2.0)

## Autofill

### What's Changed

- `get_address()` and `get_credit_card()` now throw a NoSuchRecord error instead of SqlError when the GUID is not found
- The main credit-cards table is dropped and recreated to ensure already existing databases will continue to work.

# v75.1.0 (_2021-04-13_)

## Nimbus SDK

### What's changed

- add a `get_available_experiments()` method to enable QA tooling. This should not be used for user-facing user-interfaces.

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.0.1...v75.1.0)



# v75.0.1 (_2021-04-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v75.0.0...v75.0.1)

## Nimbus SDK

### What's changed

- Make `channel` targeting comparison case-insensitive. ([#4009](https://github.com/mozilla/application-services/pull/4009))


# v75.0.0 (_2021-03-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v74.0.1...v75.0.0)



# v74.0.1 (_2021-03-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v74.0.0...v74.0.1)

## General

- Revert a Rust toolchain config change that turned out to cause build issues in our release pipeline.

# v74.0.0 (_2021-03-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v73.0.2...v74.0.0)

## General

### What's Changed

- The bundled version of Glean has been updated to v36.0.0.
- The bundled version of Nimbus has been updated to v0.10.0.

## Nimbus SDK

### ⚠️ Breaking changes ⚠️

- The new Nimbus version 0.10.0 includes new required fields `app_name` and `channel` in the `AppContext` struct passed into `initialize`.

# v73.0.2 (_2021-03-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v73.0.1...v73.0.2)

* This is a deliberately empty release, designed to help test some downstream automation
  that picks up new appservices releases.

# v73.0.1 (_2021-03-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v73.0.0...v73.0.1)

# Android

- The `-forUnitTest` build no longer includes code compiled for Windows, meaning that
  it is no longer possible to run appservices Kotlin unit tests on Windows. We hope
  this will be a temporary measure while we resolve some build issues.

# v73.0.0 (_2021-03-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v72.1.0...v73.0.0)

## General

- The bundled version of the Nimbus SDK has been updated to v0.9.0.
- The top-level Rust workspace now builds with Rust 1.50
- The top-level Rust workspace is now stapled to Rust 1.50, so all developers
  will build with 1.50, as will the continuous integration for this repo.

## iOS

- The Nimbus SDK is now available as part of the `MozillaAppServices` Swift module.

## FxA Client

### ⚠️ Breaking changes ⚠️

- The Kotlin and Swift bindings are now generated automatically using UniFFI.
  As a result many small details of the API surface have changed, such as some
  classes changing names to be consistent between Rust, Kotlin and Swift.
  ([#3876](https://github.com/mozilla/application-services/pull/3876))

# v72.1.0 (_2021-02-25_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v72.0.0...v72.1.0)

## General

### What's Changed

- This release fixes an "unsatisfied link error" problem with autofill and nimbus,
  stemming from a misconfiguration in the Android build setup.

# v72.0.0 (_2021-02-25_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v71.0.0...v72.0.0)

## General

### What's Changed

- The bundled version of the Nimbus SDK has been updated to v0.8.2.

## Autofill

### ⚠️ Breaking changes ⚠️

- The autofill Kotlin package as been renamed from `org.mozilla.appservices.autofill`
  to `mozilla.appservices.autofill`, for consistency with other components.

# v71.0.0 (_2021-02-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v70.0.0...v71.0.0)

## General

### What's Changed

- The bundled version of the Nimbus SDK has been updated to v0.8.1.

# v70.0.0 (_2021-02-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v69.0.0...v70.0.0)

## General

### What's Changed

- The bundled version of Glean has been updated to v34.0.0.
- The bundled version of the Nimbus SDK has been updated to v0.7.2.

## Autofill

### ⚠️ Breaking changes ⚠️

- The `NewCreditCardFields` record is now called `UpdatableCreditCardFields`.
- The `NewAddressFields` record is now called `UpdatableAddressFields`.

### What's Changed

- The `CreditCard` and `Address` records now exposes additional metadata around timestampes.
- Infrastructure for syncing incoming address records has been added.

# v69.0.0 (_2021-01-28_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v68.1.0...v69.0.0)

## General

### What's Changed
 - Updated nimbus-sdk to 0.7.1
 - Updated Android Components to 71.0.0

## iOS

### What's Changed
 - The `MozillaAppServices.framework` artifact now contains a dynamically-linked library
   rather than a static library.

# v68.1.0 (_2020-12-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v68.0.0...v68.1.0)

## General

### What's Changed

- The bundled version of Nimbus SDK has been updated to v0.6.4.
- The internal traits used by `sync15` have been renamed for consistency and clarity
  (and the README has been updated with docs to help explain them).
- The bundled version of Glean has been updated to v33.10.3.

# v68.0.0 (_2020-12-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v67.2.0...v68.0.0)

## General

### ⚠️ Breaking changes ⚠️

- The bundled version of Nimbus SDK has been updated to v0.6.3, which includes
  the following breaking changes:
  - Removed `NimbusClient.resetEnrollment`.
  - `NimbusClient.{updateExperiments, optInWithBranch, optOut, setGlobalUserParticipation}` now return a list of telemetry events.
    Consumers should forward these events to their telemetry system (e.g. via Glean).
  - Removed implicit fetch of experiments on first use of the database. Consumers now must
    call update_experiments explicitly in order to fetch experiments from the Remote Settings
    server.


### What's Changed

- The bundled version of Glean has been updated to v33.5.0.
- Various third-party dependencies have been updated to their latest versions.

# v67.2.0 (_2020-12-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v67.1.0...v67.2.0)

## Nimbus SDK

### What's Changed

- The bundled version of Nimbus SDK has been updated to v0.5.2.

# v67.1.0 (_2020-11-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v67.0.0...v67.1.0)

### What's Changed

- The bundled version of Glean has been updated to v33.4.0.
  (as part of [#3724](https://github.com/mozilla/application-services/pull/3724))

## Autofill

### What's New

The autofill component has a first cut at Kotlin bindings and is now bundled in
the full megazord.

# v67.0.0 (_2020-11-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v66.0.0...v67.0.0)

## General

### ⚠️ Breaking changes ⚠️

- The custom "Lockbox Megazord" package (`org.mozilla.appservices:lockbox-megazord`) has been removed.
  Existing consumers of this package who wish to update to the latest release of application-services
  should migrate to using the default `appservices:full-megazord` package, or contact the development
  team to discuss an alternate approach.
  ([#3700](https://github.com/mozilla/application-services/pull/3700))

### What's Changed

- The version of Rust used to compile our components has been pinned to v1.43.0 in order to match
  the version of Rust used in mozilla-central. Changes that do not compile under this version of
  Rust will not be accepted.
  ([#3702](https://github.com/mozilla/application-services/pull/3702))

## iOS

### What's Changed

- The bundled version of Glean has been updated to v33.1.2.
  (as part of [#3701](https://github.com/mozilla/application-services/pull/3701))

## Android

### What's Changed

- This release comes with a nontrivial increase in the compiled code size of the
  `org.mozilla.appservices:full-megazord` package, adding approximately 1M per platform
  thanks to the addition of the Nimbus SDK component.
  ([#3701](https://github.com/mozilla/application-services/pull/3701))
- Several core gradle dependencies have been updated, including gradle itself (now v6.5)
  and the android gradle plugin (now v4.0.1).
  ([#3701](https://github.com/mozilla/application-services/pull/3701))

## Nimbus SDK

### What's New

- The first version of the Nimbus Experimentation SDK is now available, via the
  `org.mozilla.appservices:nimbus` package. More details can be found in the
  [nimbus-sdk repo](https://github.com/mozilla/nimbus-sdk).
  ([#3701](https://github.com/mozilla/application-services/pull/3701))

## FxA Client

### What's Fixed

- We no longer discard the final path component from self-hosted sync tokenserver URLs.
  ([#3694](https://github.com/mozilla/application-services/pull/3694))

## Autofill

### What's Changed

- We added the `touch_address` and `touch_credit_card` store functions and refactored the component.
  ([#3691](https://github.com/mozilla/application-services/pull/3691))

## Push

### What's Changed

- Attempts to update the device push token are now rate-limited.
  ([#3673](https://github.com/mozilla/application-services/pull/3673))

## WebExtension Storage

### What's Fixed

- Syncing of incoming tombstone records has been fixed; previously the presence
  of an incoming tombstone could cause the sync to fail.
  ([#3668](https://github.com/mozilla/application-services/pull/3668))

# v66.0.0 (_2020-10-28_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v65.0.0...v66.0.0)

### Breaking changes

- Android: Updated the `getTopFrecentSiteInfos` API to specify a frecency threshold parameter for the
  fetched top frecent sites in `PlacesReaderConnection`. ([#3635](https://github.com/mozilla/application-services/issues/3635))

# v65.0.0 (_2020-10-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v64.0.0...v65.0.0)

## Android

### What's changed ###

- Upgraded the JNA dependency version to 5.6.0. ([#3647](https://github.com/mozilla/application-services/pull/3647))

## Autofill

### What's changed ###
- Updated the autofill-utils example app to include API calls for addresses. ([#3605](https://github.com/mozilla/application-services/pull/3605))
- Added API calls for credit cards. ([#3615](https://github.com/mozilla/application-services/pull/3615))

# v64.0.0 (_2020-09-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v63.0.0...v64.0.0)

## iOS

### ⚠️ Breaking changes ⚠️

- The `MozillaAppServices.framework` is now built using Xcode 12, so consumers will need
  update their own build accordingly.
  ([#3586](https://github.com/mozilla/application-services/pull/3586))

### What's changed

- The bundled version of glean has been updated to v32.4.0.
  ([#3590](https://github.com/mozilla/application-services/pull/3590))
  ()

## FxA Client

### What's changed

- Added a circuit-breaker to the `check_authorization_status` method.
  In specific circumstances, it was in possible to trigger a failure-recovery infinite loop,
  which will now error out after a certain now of retries.
  ([#3585](https://github.com/mozilla/application-services/pull/3585))

## Autofill

### What's changed ###
- Added a basic API and database layer for the autofill component. ([#3582](https://github.com/mozilla/application-services/pull/3582))

## Places

### What's changed
- Removed the duplicate Timestamp logic from Places, which now exists in Support, and updated the references.
  ([#3593](https://github.com/mozilla/application-services/pull/3593))
- Fixed a bug in bookmarks reconciliation that could lead to deleted items being resurrected
  in some circumstances.
  ([#3510](https://github.com/mozilla/application-services/pull/3510),
  [Bug 1635859](https://bugzilla.mozilla.org/show_bug.cgi?id=1635859))


## Support Code

### What's new

- The `rc_crypto` crate now supports ECDSA P384-SHA384 signature verification.
  ([#3557](https://github.com/mozilla/application-services/pull/3557))

# v63.0.0 (_2020-09-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v62.1.0...v63.0.0)

## iOS

### ⚠️ Breaking changes ⚠️

- The `MozillaAppServices.framework` build now includes Glean. Applications that were previously consuming Glean via
  its standalone framework and using `import Glean` will instead need to `import MozillaAppServices`.
  ([#3554](https://github.com/mozilla/application-services/pull/3554))

### What's changed ###

- The version of xcode used to build `MozillaAppServices.framework` has been updated to v11.7.
  ([#3556](https://github.com/mozilla/application-services/pull/3556))
- When using a custom sync tokenserver URL, the `/1.0/sync/1.5` suffix will be stripped if present.
  This should simplify setup for self-hosters who are accustomed to supplying the tokenserver URL
  in this form on other platforms.
  ([#3555](https://github.com/mozilla/application-services/pull/3555))

## Android

### What's changed ###

- `android-components` has been updated to 56.0.0 (previously 47.0.0) ([#3538](https://github.com/mozilla/application-services/pull/3538))

## Places

### What's fixed ###

- Fixed a bug where sync could ask places to recalculate the frecency of iitems that are not bookmarks,
  which would fail and prevent the sync from completing.
  ([#3567](https://github.com/mozilla/application-services/pull/3567))

### What's changed ###

- If the database somehow contains bookmarks with an invalid URL, they will now be ignored
  on read; previously invalid URLs would trigger an error and crash.
  ([#3537](https://github.com/mozilla/application-services/pull/3537))

## Push

### What's fixed ###

- Messages using the legacy `aesgcm` encryption method will how have padding bytes correctly stripped;
  previously the padding bytes would be returned as part of the message and could cause message-parsing
  errors in consuming code.
  ([#3569](https://github.com/mozilla/application-services/pull/3569))

# v62.1.0 (_2020-08-21_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v62.0.0...v62.1.0)

## Extension Storage

### What's fixed ###

- Do not check total bytes quota on storage.sync.remote operations ([Bug 1656947](https://bugzilla.mozilla.org/1656947))

## FxA Client

### What's new ###

- Send-tab metrics are recorded. A new function, `fxa_gather_telemetry` on the
  account object (exposed as `account.gatherTelemetry()` to Kotlin) which
  returns a string of JSON.

  This JSON might grow to support non-sendtab telemetry in the future, but in
  this change it has:
  - `commands_sent`, an array of objects, each with `flow_id` and `stream_id`
    string values.
  - `commands_received`, an array of objects, each with `flow_id`, `stream_id`
    and `reason` string values.

  [#3308](https://github.com/mozilla/application-services/pull/3308/)

## Places

### What's new ###

- Exclude download, redirects, reload, embed and framed link visit type from the
  `get_top_frecent_site_infos` query.
  ([#3505](https://github.com/mozilla/application-services/pull/3505))

# v62.0.0 (_2020-07-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.7...v62.0.0)

## FxA Client

### ⚠️ Breaking changes ⚠️

- Adds support for `entrypoint` in oauth flow APIs: consumers of `beginOAuthFlow` and `beginPairingFlow` (`beginAuthentication` and `beginPairingAuthentication` in ios) are now ***required*** to pass an `entrypoint` argument that would be used for metrics. This puts the `beginOAuthFlow` and `beginPairingFlow` APIs inline with other existing APIs, like `getManageAccountUrl`.  ([#3265](https://github.com/mozilla/application-services/pull/3265))
- Changes the `authorizeOAuthCode` API to now accept an `AuthorizationParams` object instead of the individual parameters. The `AuthorizationParams` also includes optional `AuthorizationPKCEParams` that contain the `codeChallenge`, `codeChallengeMethod`. `AuthorizationParams` also includes an optional `keysJwk` for requesting keys ([#3264](https://github.com/mozilla/application-services/pull/3264))

### What's new ###
- Consumers can now optionally include parameters for metrics in `beginOAuthFlow` and `beginPairingFlow` (`beginAuthentication` and `beginPairingAuthentication` in ios). Those parameters can be passed in using a `MetricsParams` struct/class. `MetricsParams` is defined in both the Kotlin and Swift bindings. The parameters are the following ([#3328](https://github.com/mozilla/application-services/pull/3328)):
  - flow_id
  - flow_begin_time
  - device_id
  - utm_source
  - utm_content
  - utm_medium
  - utm_term
  - utm_campaign
  - entrypoint_experiment
  - entrypoint_variation

## Logins

### What's fixed ###

- Fixed a bug where attempting to edit a login with an empty `form_submit_url` would incorrectly
  reject the entry as invalid ([#3331](https://github.com/mozilla/application-services/pull/3331)).

## Tabs

### What's new ###

- Tab records now have an explicit TTL set when storing on the server, to match the behaviour
  of Firefox Desktop clients ([#3322](https://github.com/mozilla/application-services/pull/3322)).

# v61.0.12 (_2020-08-07_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.11...v61.0.12)

## General

- This release only exists to correct issues that occurred when publishing a v61.0.11, which failed to produce an artifact because of a warning which occurred during the build process. (It contains a small number of additional cherry-picked patches which correct these warnings).

# v61.0.11 (_2020-08-07_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.10...v61.0.11)

## FxA Client

### What's new ###
- Send-tab metrics are recorded. A new function, `fxa_gather_telemetry` on the
  account object (exposed as `account.gatherTelemetry()` to Kotlin) which
  returns a string of JSON.

  This JSON might grow to support non-sendtab telemetry in the future, but in
  this change it has:
  - `commands_sent`, an array of objects, each with `flow_id` and `stream_id`
    string values.
  - `commands_received`, an array of objects, each with `flow_id`, `stream_id`
    and `reason` string values.

  [#3308](https://github.com/mozilla/application-services/pull/3308/)

# v61.0.10 (_2020-07-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.8...v61.0.10)

# General

- This release exists to correct an error with the publishing process that happened for v61.0.9. As a result, it's changelog is repeated below and is present in the link above.

## Logins

- Empty strings are now correctly handled in login validation. ([#3331](https://github.com/mozilla/application-services/pull/3331)).

# v61.0.9 (_2020-07-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.8...v61.0.9)

## Logins

- Empty strings are now correctly handled in login validation. ([#3331](https://github.com/mozilla/application-services/pull/3331)).

# v61.0.8 (_2020-07-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.7...v61.0.8)

## General

- The logins and places metrics have been renewed until early 2021. ([#3290](https://github.com/mozilla/application-services/pull/3290)).

# v61.0.7 (_2020-06-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.6...v61.0.7)

## General

- The default branch has been renamed from `master` to `main`

# v61.0.6 (_2020-06-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.5...v61.0.6)

## General

- Adds cargo aliases to download and use `asdev` ([#3218](https://github.com/mozilla/application-services/pull/3218))

## RustLog

- Network errors should come through as warnings and not errors. ([#3254](https://github.com/mozilla/application-services/issues/3254)).

# v61.0.5 (_2020-06-23_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.4...v61.0.5)

- No content release: we have switched to Taskgraph. ([#3168](https://github.com/mozilla/application-services/issues/3168))

# v61.0.4 (_2020-06-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.3...v61.0.4)

* Fix an issue where a node reassignment or signing out and signing back in
  wouldn't clear the locally stored last sync time for engines
  ([#3150](https://github.com/mozilla/application-services/issues/3150),
  PR [#3241](https://github.com/mozilla/application-services/pull/3241)).

# v61.0.3 (_2020-06-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.2...v61.0.3)

## Tabs

- Fix an issue which we believe is causing numerous failures deserializing protobufs ([#3214](https://github.com/mozilla/application-services/issues/3214))

# v61.0.2 (_2020-06-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.1...v61.0.2)

## FxA Client

- Short circuit requests if server requested a backoff and time period has not passed. ([#3219](https://github.com/mozilla/application-services/pull/3195))

# v61.0.1 (_2020-06-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.0...v61.0.1)

## General

- Attempt to fix some build tooling issues with the previous release; no user-visible changes. ([#3245](https://github.com/mozilla/application-services/pull/3245))


# v61.0.0 (_2020-06-15_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v60.0.0...v61.0.0)

## General

- Remove the node.js integration tests helper and removes node from the circleci environment. ([#3187](https://github.com/mozilla/application-services/pull/3187))
- Put `backtrace` behind a cargo feature. ([#3213](https://github.com/mozilla/application-services/pull/3213))
- Move sqlite dependency down from rc_crypto to nss_sys. ([#3198](https://github.com/mozilla/application-services/pull/3198))
- Adds jwe encryption in scoped_keys. ([#3195](https://github.com/mozilla/application-services/pull/3195))
- Adds an implementation for [pbkdf2](https://www.ietf.org/rfc/rfc2898.txt). ([#3193](https://github.com/mozilla/application-services/pull/3193))
- Fix bug to correctly return the given defaults when the storageArea's `get()` method is used with an empty store ([bug 1645598](https://bugzilla.mozilla.org/show_bug.cgi?id=1645598)). ([#3236](https://github.com/mozilla/application-services/pull/3236))
- Fixed a sync bug where the application not providing the "persisted state" would mean the declined list was handled incorrectly ([#3205](https://github.com/mozilla/application-services/issues/3205))

## Android

- From now on the project uses the Android SDK manager side-by-side NDK. ([#3222](https://github.com/mozilla/application-services/pull/3222))
  - Download the new NDK by running `./verify-android-environment.sh` in the `libs` directory.
    - Alternatively, you can download the NDK in Android Studio by going in Tools > SDK Manager > SDK Tools > NDK (check Show Package Details) and check `21.3.6528147`.
  - The `ANDROID_NDK_ROOT`, `ANDROID_NDK_HOME` environment variables (and the directory they point to) can be removed.

# v60.0.0 (_2020-06-01_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.59.0...v60.0.0)

## General

- Remove `failure` from the sync_tests and replace it with `anyhow`. ([#3188](https://github.com/mozilla/application-services/pull/3188))

- Adds an alias for generating protobuf files, you can now use `cargo regen-protobufs` to generate them. ([#3178](https://github.com/mozilla/application-services/pull/3178))

- Replaced `failure` with `anyhow` and `thiserror`. ([#3132](https://github.com/mozilla/application-services/pull/3132))

- Android: Added `getTopFrecentSiteInfos` API to retrieve a list of the top frecent sites in `PlacesReaderConnection`. ([#2163](https://github.com/mozilla/application-services/issues/2163))

## FxA Client

### What's new

- Additional special case for China FxA in `getPairingAuthorityURL`. ([#3160](https://github.com/mozilla/application-services/pull/3160))
- Silently ignore push messages for unrecognized commands, rather than reporting an error. ([#3177](https://github.com/mozilla/application-services/pull/3177))

# v0.59.1 (_2021-01-26_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.59.0...v0.59.1)

## General

- Our iOS framework binaries are now built using XCode 11.7. ([#3833](https://github.com/mozilla/application-services/issues/3833))

# v0.59.0 (_2020-05-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.58.2...v0.59.0)

## Viaduct

### Breaking changes

- The `include_cookies` setting is not supported anymore (was `false` by default). ([#3096](https://github.com/mozilla/application-services/pull/3096))

## FxA Client

- Added option boolean argument `ignoreCache` to ignore caching for `getDevices`. ([#3066](https://github.com/mozilla/application-services/pull/3066))

### ⚠️ Breaking changes ⚠️
- iOS: Renamed `fetchDevices(forceRefresh)` to `getDevices(ignoreCache)` to establish parity with Android. ([#3066](https://github.com/mozilla/application-services/pull/3066))
- iOS: Renamed argument of `fetchProfile` from `forceRefresh` to `ignoreCache`. ([#3066](https://github.com/mozilla/application-services/pull/3066))

# v0.58.3 (_2020-05-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.58.2...v0.58.3)

- Backported the following Send Tab fixes: [#3065](https://github.com/mozilla/application-services/pull/3065) [#3084](https://github.com/mozilla/application-services/pull/3084) to `v0.58.2`. ([#3101](https://github.com/mozilla/application-services/pull/3101))

# v0.58.2 (_2020-04-29_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.58.1...v0.58.2)

## General

- Android: An Android 5 and 6 bug related to protobufs is now fixed. ([#3054](https://github.com/mozilla/application-services/pull/3054))

# v0.58.1 (_2020-04-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.58.0...v0.58.1)

## General

- Android: A bug in the protobuf library that made the previous version unusable has been fixed. ([#3033](https://github.com/mozilla/application-services/pull/3033))

# v0.58.0 (_2020-04-22_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.57.0...v0.58.0)

## General

- Android: Gradle wrapper version upgraded to `6.3`, Android Gradle Plugin version upgraded to `3.6.0`. ([#2917](https://github.com/mozilla/application-services/pull/2917))
- Android: Upgraded NDK from r20 to r21. ([#2985](https://github.com/mozilla/application-services/pull/2985))
- iOS: Xcode version changed to 11.4.1 from 11.4.0. ([#2996](https://github.com/mozilla/application-services/pull/2996))

## FxA Client

- iOS: `refreshProfile` now takes an optional boolean argument `forceRefresh` to force a network request to be made in every case ([#3000](https://github.com/mozilla/application-services/pull/3000))
- Added an optional `ttl` parameter to `getAccessToken` to limit the lifetime of the token. ([#2896](https://github.com/mozilla/application-services/pull/2896))

# v0.57.0 (_2020-03-31_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.56.0...v0.57.0)

## General

### ⚠️ Breaking changes ⚠️

- iOS: The `reqwest` network stack will not be initialized automatically anymore.
Please call `Viaduct.shared.useReqwestBackend()` as soon as possible before using the framework. ([#2880](https://github.com/mozilla/application-services/pull/2880))

## Logins

### What's New

- A new function was added to return a list of duplicate logins, ignoring
  username. ([#2542](https://github.com/mozilla/application-services/pull/2542))

# v0.56.0 (_2020-03-26_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.55.0...v0.56.0)

## General

### What's changed

- iOS: Xcode version changed to 11.4.0 from 11.3.1.

## Logins

### ⚠️ Breaking changes ⚠️

- Android: `MemoryLoginsStorage` has been removed. Use DatabaseLoginsStorage(":memory:") instead.
  ([#2833](https://github.com/mozilla/application-services/pull/2823)).

## Libs

### What's changed

- The project now builds with version 4.3.0 of SQLCipher instead of a fork
  of version 4.2.0. Newest version has NSS crypto backend. ([#2822](https://github.com/mozilla/application-services/pull/2822)).

## FxA Client

### Breaking changes

- `Server.dev` is now `Server.stage` to reflect better the FxA server instance it points to. ([#2830](https://github.com/mozilla/application-services/pull/2830)).

# v0.55.3 (_2020-04-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.55.2...v0.55.3)

## Places

- Fix table name for history migration

# v0.55.2 (_2020-04-14_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.55.1...v0.55.2)

## Places

- Android: Fennec's bookmarks db version supported by the migrations is now the same as that of history

# v0.55.1 (_2020-04-14_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.55.0...v0.55.1)

## Places

- Android: Fennec migrations for history and bookmarks now support Fennec database versions 34 and 23, respectively. ([#2949](https://github.com/mozilla/application-services/pull/2949))

# v0.55.0 (_2020-03-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.54.1...v0.55.0)

## Places

### ⚠️ Breaking changes ⚠️

- Android: `PlacesConnection.deletePlace` has been renamed to
  `deleteVisitsFor`, to clarify that it might not actually delete the
  page if it's bookmarked, or has a keyword or tags
  ([#2695](https://github.com/mozilla/application-services/pull/2695)).

### What's fixed

- `history::delete_visits_for` (formerly `delete_place_by_guid`) now correctly
  deletes all visits from a page if it has foreign key references, like
  bookmarks, keywords, or tags. Previously, this would cause a constraint
  violation ([#2695](https://github.com/mozilla/application-services/pull/2695)).

## FxA Client

### What's new

- Added `getPairingAuthorityURL` method returning the URL the user should navigate to on their Desktop computer to perform a pairing flow. ([#2815](https://github.com/mozilla/application-services/pull/2815))

### Breaking changes

- In order to account better for self-hosted FxA/Sync backends, the FxAConfig objects have been reworked. ([#2801](https://github.com/mozilla/application-services/pull/2801))
  - iOS: `FxAConfig.release(contentURL, clientID)` is now `FxAConfig(server: .release, contentURL, clientID)`.
  - Android: `Config.release(contentURL, clientID)` is now `Config(Server.RELEASE, contentURL, clientID)`.
  - These constructors also take a new `tokenServerUrlOverride` optional 4th parameter that overrides the token server URL.

- iOS: `FxAccountManager`'s `getManageAccountURL` and `getTokenServerEndpointURL` methods now run on background thread and return their results in a callback function. ([#2813](https://github.com/mozilla/application-services/pull/2813))

# v0.54.1 (_2020-03-12_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.54.0...v0.54.1)

## Sync

### What's fixed

- Engine disabled/enabled state changes now work again after a regression in
  0.53.0.

## Android

### What's changed

- There is now preliminary support for an "autoPublish" local-development workflow similar
  to the one used when working with Fenix and android-components; see
  [this howto guide](./docs/howtos/locally-published-components-in-fenix.md) for details.

## Places

### What's fixed

- Improve handling of bookmark search keywords. Keywords are now imported
  correctly from Fennec, and signing out of Sync in Firefox for iOS no longer
  loses keywords ([#2501](https://github.com/mozilla/application-services/pull/2501)).

# v0.54.0 (_2020-03-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.53.2...v0.54.0)

## General

### What's changed

- iOS: Xcode version changed to 11.3.1 from 11.3.0.

## Rust

### What's New

- Sourcing `libs/bootstrap-desktop.sh` is not a thing anymore. Please run `./libs/verify-desktop-environment.sh` at least once instead. ([#2769](https://github.com/mozilla/application-services/pull/2769))

## Push

### Breaking changes

- Android: The `PushManager.verifyConnection` now returns a `List<PushSubscriptionChanged>` that contain the channel ID and scope of the subscriptions that have expired. ([#2632](https://github.com/mozilla/application-services/pull/2632))
  See [`onpushsubscriptionchange`][0] events on how this change can be propagated to notify web content.

[0]: https://developer.mozilla.org/en-US/docs/Web/API/ServiceWorkerGlobalScope/onpushsubscriptionchange

## Places

### What's fixed

- Improve handling of tags for bookmarks with the same URL. These bookmarks no
  longer cause syncs to fail ([#2750](https://github.com/mozilla/application-services/pull/2750)),
  and bookmarks with duplicate or mismatched tags are reuploaded
  ([#2774](https://github.com/mozilla/application-services/pull/2774)).

### Breaking changes

- Synced items with unknown types now fail the sync, instead of being silently
  ignored. We'll monitor this error in telemetry, and add logic to delete these
  items on the server if needed
  ([#2780](https://github.com/mozilla/application-services/pull/2780)).

# v0.53.2 (_2020-03-05_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.53.1...v0.53.2)

## FxA Client

### What's fixed

- iOS: `FxAccountManager.logout` will now properly clear the persisted account state. ([#2755](https://github.com/mozilla/application-services/issues/2755))
- iOS: `FxAccountManager.getAccessToken` now runs in a background thread. ([#2755](https://github.com/mozilla/application-services/issues/2755))

# v0.53.1 (_2020-03-05_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.53.0...v0.53.1)

## Android

### What's changed

- A megazord loading failure will throw as soon as possible rather than at call time.
  ([#2739](https://github.com/mozilla/application-services/issues/2739))

## iOS

### What's New

- Developers can now run `./libs/verify-ios-environment.sh` to ensure their machine is ready to build the iOS Xcode project smoothly. ([#2737](https://github.com/mozilla/application-services/pull/2737))

## FxA Client

### What's new

- Added `FxAConfig.china` helper function to use FxA/Sync chinese servers. ([#2736](https://github.com/mozilla/application-services/issues/2736))
- iOS: Added `FxAccountManager.handlePasswordChanged` method that should be called after... a password change! ([#2744](https://github.com/mozilla/application-services/issues/2744))

# v0.53.0 (_2020-02-27_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.52.0...v0.53.0)

## Megazords

### What's changed

- The fenix megazord is no more! ([#2565](https://github.com/mozilla/application-services/pull/2565))

  The full megazord should be used instead. The two are functionally identical,
  however this should reduce the configuration surface required to use the
  application-services code.

  An example PR showing the changes typically required for this is available
  here: https://github.com/MozillaReality/FirefoxReality/pull/2867. Please feel
  free to reach out if you have any issues.

## Sync

### What's fixed

- Rust sync code is now more robust in the face of corrupt meta/global
  records. ([#2688](https://github.com/mozilla/application-services/pull/2688))

- In v0.52.0 we reported some network related fixes. We lied. This time
  we promise they are actually fixed. ([#2616](https://github.com/mozilla/application-services/issues/2616),
  [#2617](https://github.com/mozilla/application-services/issues/2617)
  [#2623](https://github.com/mozilla/application-services/issues/2623))

### What's changed

- Fewer updates to the 'clients' collection will be made. ([#2624](https://github.com/mozilla/application-services/issues/2624))

## FxA Client

### What's changed

- The `ensureCapabilities` method will not perform any network requests if the
  given capabilities are already registered with the server.
  ([#2681](https://github.com/mozilla/application-services/pull/2681))

### What's fixed

- Ensure an offline migration recovery succeeding does not happen multiple times.
  ([#2706](https://github.com/mozilla/application-services/pull/2706))

## Places

### What's fixed

- `storage::history::apply_observation` and `storage::bookmarks::update_bookmark`
  now flush pending origin and frecency updates. This fixes a bug where origins
  might be flushed at surprising times, like right after clearing history.
  ([#2693](https://github.com/mozilla/application-services/issues/2693))

## Push

### What's fixed

- `PushManager.dispatchInfoForChid` does not throw `KotlinNullPointerException` anymore if the method returned nothing. ([#2703](https://github.com/mozilla/application-services/issues/2703))


# v0.52.0 (_2020-02-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.51.1...v0.52.0)

## Sync

### What's changed

- Better caching of the tokenserver token and info/configuration response. ([#2616](https://github.com/mozilla/application-services/issues/2616))

- Less network requests will be made in the case nothing has changed on the server. ([#2623](https://github.com/mozilla/application-services/issues/2623))

## Places

### What's changed

- Added a new field `reasons` which is a `List` of `SearchResultReason`s in `SearchResult`. ([#2564](https://github.com/mozilla/application-services/pull/2564))

- Some places import related issues fixed ([#2536](https://github.com/mozilla/application-services/issues/2536),
  [#2607](https://github.com/mozilla/application-services/issues/2607))

### Breaking changes

- Android: The `PlacesWriterConnection.resetHistorySyncMetadata` and `PlacesWriterConnection.resetBookmarkSyncMetadata` methods have been moved to the `PlacesApi` class. ([#2668](https://github.com/mozilla/application-services/pull/2668))
- iOS: The `PlacesWriteConnection.resetHistorySyncMetadata` method has been moved to the `PlacesAPI` class. ([#2668](https://github.com/mozilla/application-services/pull/2668))

## FxA Client

### What's New

- Android: `FirefoxAccount.handlePushMessage` now handles all possible FxA push payloads and will return new `AccountEvent`s ([#2522](https://github.com/mozilla/application-services/pull/2522)):
  - `.ProfileUpdated` which should be handled by fetching the newest profile.
  - `.AccountAuthStateChanged` should be handled by checking if the authentication state is still valid.
  - `.AccountDestroyed` should be handled by removing the account information (no need to call `FirefoxAccount.disconnect`) from the device.
  - `.DeviceConnected` can be handled by showing a "<Device name> is connected to this account" notification.
  - `.DeviceDisconnected` should be handled by showing a "re-auth" state to the user if `isLocalDevice` is true. There is no need to call `FirefoxAccount.disconnect` as it will fail.

- iOS: Added `FxAccountManager.getSessionToken`. Note that you should request the `.session` scope in the constructor for this to work properly. ([#2638](https://github.com/mozilla/application-services/pull/2638))
- iOS: Added `FxAccountManager.getManageAccountURL`. ([#2658](https://github.com/mozilla/application-services/pull/2658))
- iOS: Added `FxAccountManager.getTokenServerEndpointURL`. ([#2658](https://github.com/mozilla/application-services/pull/2658))
- iOS: Added migration methods to `FxAccountManager` ([#2637](https://github.com/mozilla/application-services/pull/2637)):
  - `authenticateViaMigration` will try to authenticate an account without any user interaction using previously stored account information.
  - `accountMigrationInFlight` and `retryMigration` should be used in conjunction to handle cases where the migration could not be completed but is still recoverable.
- Added a `deviceId` property to the `AccountEvent.deviceDisconnected` enum case. ([#2645](https://github.com/mozilla/application-services/pull/2645))
- Added `context=oauth_webchannel_v1` in `getManageDevicesURL` methods for WebChannel redirect URLs. ([#2658](https://github.com/mozilla/application-services/pull/2658))

### Breaking changes

- Android: A few changes were made in order to decouple device commands from "account events" ([#2522](https://github.com/mozilla/application-services/pull/2522)):
  - The `AccountEvent` enum has been refactored: `.TabReceived` has been replaced by `.IncomingDeviceCommand(IncomingDeviceCommand)`, `IncomingDeviceCommand` itself is another enum that contains `TabReceived`.
  - `FirefoxAccount.pollDeviceCommands` now returns an array of `IncomingDeviceCommand`.

- iOS: The `FxaAccountManager` class has been renamed to `FxAccountManager`. ([#2637](https://github.com/mozilla/application-services/pull/2637))
- iOS: The `FxAccountManager` default applications scopes do not include `.oldSync` anymore. ([#2638](https://github.com/mozilla/application-services/pull/2638))
- iOS: `FxaAccountManager.getTokenServerEndpointURL` now returns the full token server URL such as `https://token.services.mozilla.com/1.0/sync/1.5`. ([#2676](https://github.com/mozilla/application-services/pull/2676))

## Push

### What's New

- Android: Exposed `GeneralError` to the Kotlin layer.

# v0.51.1 (_2020-02-07_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.51.0...v0.51.1)

## Android

### What's new

- Updated android gradle plugin version to 3.5.3 ([#2600](https://github.com/mozilla/application-services/pull/2600))


# v0.51.0 (_2020-02-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.50.2...v0.51.0)

## FxA Client

### What's New

- `FirefoxAccount` is now deprecated ([#2454](https://github.com/mozilla/application-services/pull/2454)).
- Introducing `FxAccountManager` which provides a higher-level interface to Firefox Accounts. Among other things, this class handles (and can recover from) authentication errors, exposes device-related account methods, handles its own keychain storage and fires observer notifications for important account events ([#2454](https://github.com/mozilla/application-services/pull/2454)).

### Breaking changes

- `FirefoxAccount.fromJSON(json: String)` has been replaced by the `FirefoxAccount(fromJsonState: String)` constructor ([#2454](https://github.com/mozilla/application-services/pull/2454)).


# v0.50.2 (_2020-02-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.50.1...v0.50.2)

### What's changed

- Re-releasing to fix misconfigured build options in v0.50.1.


# v0.50.1 (_2020-02-06_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.50.0...v0.50.1)

## FxA Client

### What's changed

- Fixed a potentially-unsafe use of a boolean in the FFI interface for `migrateFromSessionToken`.
  ([#2592](https://github.com/mozilla/application-services/pull/2592)).


# v0.50.0 (_2020-02-05_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.49.0...v0.50.0)

## FxA Client

### What's changed

- Android: `migrateFromSessionToken` now correctly persists in-flight migration state even
  when throwing an error ([#2586](https://github.com/mozilla/application-services/pull/2586)).

### Breaking changes

- `isInMigrationState` now returns an enum rather than a boolean, to indicate whether
  the migration will re-use or duplicate the underlying sessionToken.
  ([#2586](https://github.com/mozilla/application-services/pull/2586))


# v0.49.0 (_2020-02-03_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.3...v0.49.0)

## FxA Client

### What's New

- Android: `migrateFromSessionToken` now handles offline use cases. It caches the data the consumers
  originally provide. If there's no network connectivity then the migration could be retried using the
  new `retryMigrateFromSessionToken` method. Consumers may also use the `isInMigrationState` method
  to check if there's a migration in progress. ([#2492](https://github.com/mozilla/application-services/pull/2492))

### Breaking changes

- `migrateFromSessionToken` now returns a metrics JSON object if the migration succeeded.
  ([#2492](https://github.com/mozilla/application-services/pull/2492))


# v0.48.3 (_2020-01-23_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.2...v0.48.3)

## Places

## What's new

- The Dogear library for merging synced bookmarks has been updated to the latest version.
  ([#2469](https://github.com/mozilla/application-services/pull/2469))
- Places now exposes `resetHistorySyncMetadata` and `resetBookmarkSyncMetadata`
  methods, which cleans up all Sync state, including tracking flags and change
  counters. These methods should be called by consumers when the user signs out,
  to avoid tracking changes and causing unexpected behavior the next time they
  sign in.
  ([#2447](https://github.com/mozilla/application-services/pull/2447))

## What's changed

- Ensure we do the right thing with timestamps, tags and keywords on first sync after migration.
  ([#2472](https://github.com/mozilla/application-services/pull/2472))
- Don't count Fennec bookmarks we know we don't import in success metrics.
  ([#2488](https://github.com/mozilla/application-services/pull/2488))
- Don't fail if the Fennec database has negative positions when importing top-sites
  ([#2462](https://github.com/mozilla/application-services/pull/2462))
- Fix issue with bookmark tags when syncing
  ([#2480](https://github.com/mozilla/application-services/pull/2480))
- Quality and usage metrics are recorded (ditto for logins)
- Fix some swift warnings
  ([#2491](https://github.com/mozilla/application-services/pull/2491))

### Breaking Changes

- The Android bindings now collect some basic performance and quality metrics via Glean.
  Applications that submit telemetry via Glean must request a data review for these metrics
  before integrating the places component. See the component README.md for more details.
  ([#2431](https://github.com/mozilla/application-services/pull/2431))
- iOS only: `PlacesAPI.resetBookmarksMetadata` has been renamed to
  `PlacesAPI.resetBookmarkSyncMetadata`, for consistency with history. The functionality
  remains the same.


# v0.48.2 (_2020-01-13_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.1...v0.48.2)

## FxA Client

### What's changed

* Fixed a bug in deserializing FxA objects from JSON when the new `introspection_endpoint`
  field is not present.


# v0.48.1 (_2020-01-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.0...v0.48.1)

## General

- Revert NSS to version 3.46.

## Logins

### What's changed

* The error strings returned by `LoginsStorage.importLogins` as part of the migration metrics bundle,
  no longer include potentially-sensitive information such as guids.

# v0.48.0 (_2020-01-03_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.47.0...v0.48.0)

## Logins

### Breaking Changes

- `LoginsStorage.importLogins` returns logins migration metrics as JSON object. ([#2382](https://github.com/mozilla/application-services/issues/2382))

- iOS only: Added a migration path for apps to convert the encrypted database headers to plaintext([#2100](https://github.com/mozilla/application-services/issues/2100)).
New databases must be opened using `LoginsStorage.unlockWithKeyAndSalt` instead of `LoginsStorage.unlock` which is now deprecated.
To migrate current users databases, it is required to call `LoginsStorage.migrateToPlaintextHeader` before opening the database. This new method requires a salt. The salt persistence is now the responsibility of the application, which should be stored alongside the encryption key. For an existing database, the salt can be obtained using `LoginsStorage.getDbSaltForKey`.

### What's new

- Android: Added ability to rekey the database via `rekeyDatabase`. [[#2228](https://github.com/mozilla/application-services/pull/2228)]

## FxA Client

### Breaking Changes

* Android: `migrateFromSessionToken` now reuses the existing 'sessionToken' instead of creating a new session token.

### What's new

* Android: New method `copyFromSessionToken` will create a new 'sessionToken' state, this is what `migrateFromSessionToken` used to do,
before this release.

## Places

### Breaking Changes

- - `PlacesApi.importVisitsFromFennec` return history migration metrics as JSON object. ([#2414](https://github.com/mozilla/application-services/issues/2414))
- - `PlacesApi.importBookmarksFromFennec` no longer returns pinned bookmarks, it now returns migration metrics as JSON object. ([#2427](https://github.com/mozilla/application-services/issues/2427))

### What's new

* Android: New method `PlacesApi.importPinnedSitesFromFennec` returns a list of pinned bookmarks from Fennec. ([#2427](https://github.com/mozilla/application-services/issues/2427))

# v0.47.0 (_2019-12-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.46.0...v0.47.0)

## General

- Updated NSS to version 3.48. ([#2379](https://github.com/mozilla/application-services/issues/2379))
- Our iOS framework is now built using Swift version 5.0. ([#2383](https://github.com/mozilla/application-services/issues/2383))
- Our iOS framework binaries are now built using XCode 11.3. ([#2383](https://github.com/mozilla/application-services/issues/2383))

## Logins

### Breaking Changes

- `LoginsStorage.getByHostname` has been removed. ([#2152](https://github.com/mozilla/application-services/issues/2152))

### What's new

- `LoginsStorage.getByBaseDomain` has been added. ([#2152](https://github.com/mozilla/application-services/issues/2152))
- Removed hard deletion of `SyncStatus::New` records in `delete` and `wipe` logins database functions. ([#2362](https://github.com/mozilla/application-services/pull/2362))
- Android: The `MemoryLoginsStorage` class has been deprecated, because it behaviour has already started to
  diverge from that of `DatabaseLoginStorage`. To replace previous uses of this class in tests, please either
  explicitly mock the `LoginsStorage` interface or use a `DatabaseLoginStorage` with a tempfile or `":memory:"`
  as the `dbPath` argument. ([#2389](https://github.com/mozilla/application-services/issues/2389))

# v0.46.0 (_2019-12-12_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.45.1...v0.46.0)

## Logins

### Breaking Changes

- The Android bindings now collect some basic performance and quality metrics via Glean.
  Applications that submit telemetry via Glean must request a data review for these metrics
  before integrating the logins component. See the component README.md for more details.
  ([#2225](https://github.com/mozilla/application-services/pull/2225))
- `username`, `usernameField`, and `passwordField` are no longer
  serialized as `null` in the case where they are empty strings. ([#2252](https://github.com/mozilla/application-services/pull/2252))
  - Android: `ServerPassword` fields `username`, `usernameField`, and
    `passwordField` are now required fields -- `null` is not acceptable,
    but empty strings are OK.
  - iOS: `LoginRecord` fields `username`, `usernameField` and
    `passwordField` are no longer nullable.

# v0.45.1 (_2019-12-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.45.0...v0.45.1)

This release exists only to rectify a publishing error that occurred with v0.45.0.

# v0.45.0 (_2019-12-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.44.0...v0.45.0)

## Places

### What's new

- Added `PlacesReaderConnection.getVisitPageWithBound` which performs
  faster history visits pagination by first skipping directly to a `bound` timestamp and then
  skipping over `offset` items from `bound`. ([#1019](https://github.com/mozilla/application-services/issues/1019))

## Push

### Breaking Changes

- `PushManager.decrypt` will now throw a `RecordNotFoundError` exception instead of `StorageError` if a matching subscription could not be found. ([#2355](https://github.com/mozilla/application-services/pull/2355))

## FxA Client

### What's new

- `FirefoxAccount.checkAuthorizationStatus` will check the status of the currently stored refresh token. ([#2332](https://github.com/mozilla/application-services/pull/2332))

## Logins

### Breaking Changes

- Login records with a `httpRealm` attribute will now have their `usernameField` and `passwordField`
  properties silently cleared, to help ensure data consistency. ([#2158](https://github.com/mozilla/application-services/pull/2158))

### What's new

- Added invalid character checks from Desktop to `LoginsStorage.ensureValid` and introduced `INVALID_LOGIN_ILLEGAL_FIELD_VALUE` error. ([#2262](https://github.com/mozilla/application-services/pull/2262))

## Sync Manager

### Breaking Changes

- When asked to sync all engines, SyncManager will now sync all engines for which a handle has been set.
  Previously it would sync all known engines, panicking if a handle had not been set for some engine.
  While *technically* a breaking change, we expect that the new behaviour is almost certainly what
  consuming applications actually want in practice. ([#2313](https://github.com/mozilla/application-services/pull/2313))

# v0.44.0 (_2019-11-21_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.43.1...v0.44.0)

## Logins

### What's new

- Added ability to prevent insertion/updates from creating dupes via `LoginsStorage.ensureValid`. ([#2101](https://github.com/mozilla/application-services/pull/2101))

## Tabs

### Breaking Changes

- The `RemoteTabsProvider` class constructor does not take the `localDeviceId` argument anymore.
- The `RemoteTabsProvider.sync` method takes a `localDeviceId` argument.

# v0.43.1 (_2019-11-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.43.0...v0.43.1)

This release exists only to rectify a publishing error that occurred with v0.43.0.

# v0.43.0 (_2019-11-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.4...v0.43.0)

This release exists only to rectify a publishing error that occurred with v0.42.4.

# v0.42.4 (_2019-11-18_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.3...v0.42.4)

## General

### What's New

- Synced Tabs is available as an Android component in the `org.mozilla.appservices.experimental.remotetabs` maven package.

## Push Client

### What's new

- `PushManager.dispatchInfoForChid(channelID)` now also returns the
  `endpoint` and `appServerKey` from the subscription.

### Breaking Changes

- The `appServerKey` VAPID public key has moved from `PushConfig` to
  `PushManager.subscription(channelID, scope, appServerKey)`.

- The unused `regenerate_endpoints()` function has been removed.

# v0.42.3 (_2019-11-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.2...v0.42.3)

## General

### What's New

- On Android, our Megazord libraries now include license information for dependencies
  as part of their `.pom` file, making it easily available to tools such as the
   [oss-licenses-plugin](https://github.com/google/play-services-plugins/tree/master/oss-licenses-plugin)

- Our iOS framework binaries are now built using XCode 11.2.

# v0.42.2 (_2019-10-21_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.1...v0.42.2)

## Places

### What's new

- Android: Exposed `storage::bookmarks::erase_everything`, which deletes all bookmarks without affecting history, through FFI. ([#2012](https://github.com/mozilla/application-services/pull/2012))

## FxA Client

### What's new

- Android: Add ability to get an OAuth code using a session token via the `authorizeOAuthCode` method. ([#2003](https://github.com/mozilla/application-services/pull/2003))


# v0.42.1 (_2019-10-21_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.42.0...v0.42.1)

## Places

### What's new

- Android: The Fennec bookmarks import method (`importBookmarksFromFennec`) will now return a list of pinned bookmarks. ([#1993](https://github.com/mozilla/application-services/pull/1993))

# v0.42.0 (_2019-10-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.41.0...v0.42.0)

## Places

### What's new

- Android: Fennec history import now supports microsecond timestamps for `date_visited`.

### Breaking changes

- The methods for importing places data from fenix have been moved from the writer connection to the PlacesAPI.

# v0.41.0 (_2019-10-02_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.40.0...v0.41.0)

## General

### What's New

- Our components are now built with the newer Android NDK r20 instead of r15c. This change will make it easier for contributors to set up their development environment since there's no need to generate Android toolchains anymore. ([#1916](https://github.com/mozilla/application-services/pull/1916))
For existing contributors, here's what you need to do immediately:
  - Download and extract the [Android NDK r20](https://developer.android.com/ndk/downloads).
  - Change the `ANDROID_NDK_ROOT` and `ANDROID_NDK_HOME` environment variables to point to the newer NDK dir. You can also delete the now un-used `ANDROID_NDK_TOOLCHAIN_DIR` variable.
  - Delete `.cargo/config` at the root of the repository if you have it.
  - Regenerate the Android libs: `cd libs && rm -rf android && ./build-all.sh android`.

## Logins

### What's new

- Added ability to get logins by hostname by using `LoginsStorage.getByHostname`. ([#1782](https://github.com/mozilla/application-services/pull/1782))

# v0.40.0 (_2019-09-26_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.39.4...v0.40.0)

## Logins

### Breaking Changes

- getHandle has been moved to the LoginsStorage interface. All implementers other than DatabaseLoginsStorage should implement this by throwing a `UnsupportedOperationException`.

# v0.39.4 (_2019-09-25_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.39.3...v0.39.4)

## Sync Manager

### What's fixed

- Engines which are disabled will not have engine records in meta/global. ([#1866](https://github.com/mozilla/application-services/pull/1866))
- The FxA access token is no longer logged at the debug level. ([#1866](https://github.com/mozilla/application-services/pull/1866))

# v0.39.3 (_2019-09-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.39.2...v0.39.3)

## FxA Client

### What's new

- The OAuth access token cache is now persisted as part of the account state data,
  which should reduce the number of times callers need to fetch a fresh access token
  from the server.

# v0.39.2 (_2019-09-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.39.1...v0.39.2)

## Sync Manager

### What's fixed

- Clients with missing engines in meta/global should have the engines repopulated. ([#1847](https://github.com/mozilla/application-services/pull/1847))

# v0.39.1 (_2019-09-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.39.0...v0.39.1)

## FxA Client

### What's new

Add ability to get the current device id in Kotlin via `getCurrentDeviceId` method.

# v0.39.0 (_2019-09-17_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.38.2...v0.39.0)

## FxA Client

### What's new

* New `getSessionToken` method on the FxA Client that returns the stored session_token from state.
Also we now store the session_token into the state from the 'https://identity.mozilla.com/tokens/session' scope.

## Places

### What's fixed

- Hidden URLs (redirect sources, or links visited in frames) are no longer
  synced or returned in `get_visit_infos` or `get_visit_page`. Additionally,
  a new `is_hidden` flag is added to `HistoryVisitInfo`, though it's currently
  always `false`, since those visits are excluded.
  ([#1715](https://github.com/mozilla/application-services/pull/1715))

## Sync Manager

- The new sync manager component is now available for integration ([#1447](https://github.com/mozilla/application-services/pull/1447)).
    - This should include no breaking changes at the moment, but in the future
      we will deprecate the non-sync manager sync APIs on android.
    - Note: Currently, the sync manager is only available in the `full` and
      `fenix` megazords.

# v0.38.2 (_2019-09-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.38.1...v0.38.2)

## Android

### What's new

- The Gradle Android Plugin has been updated to 3.5.0. ([#1680](https://github.com/mozilla/application-services/pull/1680))

## iOS

### What's new

- Releases are now built with Xcode 11.0.0. ([#1719](https://github.com/mozilla/application-services/pull/1719))

# v0.38.1 (_2019-08-26_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.38.0...v0.38.1)

## FxA Client

### What's new

-  Added support for a webchannel redirect behaviour. ([#1608](https://github.com/mozilla/application-services/pull/1608))

## Android

### What's new

- Initial versions of Fennec data import methods have landed:
  - Bookmarks and history visits can be imported by calling `PlacesWriterConnection.importBookmarksFromFennec` and `PlacesWriterConnection.importVisitsFromFennec` respectively. ([#1595](https://github.com/mozilla/application-services/pull/1595), [#1461](https://github.com/mozilla/application-services/pull/1461))
  - Logins can be imported with `LoginsStorage.importLogins`. ([#1614](https://github.com/mozilla/application-services/pull/1614))

# v0.38.0 (_2019-08-19_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.37.1...v0.38.0)

## General

- Our OpenSSL dependency has been removed for all platforms other than
  desktop-linux (used when running local rust unit tests and the android
  -forUnitTests artifact). All other platforms use NSS.
  ([#1570](https://github.com/mozilla/application-services/pull/1570))

## Places

### What's Fixed

* Tags containing embedded whitespace are no longer marked as invalid and
  removed. ([#1616](https://github.com/mozilla/application-services/issues/1616))

# v0.37.1 (_2019-08-09_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.37.0...v0.37.1)

## Android

### What's fixed

- Published artifacts should now correctly declare their `packaging` type in
  their pom files. ([#1564](https://github.com/mozilla/application-services/pull/1564))

## FxA Client

### What's fixed

- `FirefoxAccount.handlePushMessage` will not return an error on unknown push payloads.

# v0.37.0 (_2019-08-08_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.36.0...v0.37.0)

## FxA Client

### What's new

- The Tablet, VR and TV devices types have been added.

### What's fixed

- The `FirefoxAccount.disconnect` method should now properly dispose of the associated device record.

### Breaking changes

- The `FirefoxAccount.beginOAuthFlow` method does not require the `wantsKeys` argument anymore
  as it will always do the right thing based on the requested scopes.

# v0.36.0 (_2019-07-30_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.35.4...v0.36.0)

## General

### What's New

- The Fenix megazord now supports Logins. ([#1465](https://github.com/mozilla/application-services/pull/1465))

- For maintainers only: please delete the `libs/{desktop, ios, android}` folders and start over using `./build-all.sh [android|desktop|ios]`.

### What's fixed

- Android x86_64 crashes involving the `intel_aes_encrypt_cbc_128` missing symbol have been fixed. ([#1495](https://github.com/mozilla/application-services/pull/1495))

## Places

### What's New

- Added a `getBookmarkURLForKeyword` method that retrieves a URL associated to a keyword. ([#1345](https://github.com/mozilla/application-services/pull/1345))

## Push

### Breaking changes

- `PushManager.dispatchForChid` method has been renamed to `dispatchInfoForChid` and its result type is now Nullable. ([#1490](https://github.com/mozilla/application-services/pull/1490))

# v0.35.4 (_2019-07-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.35.3...v0.35.4)

This release exists only to rectify a publishing error that occurred with v0.35.3.

# v0.35.3 (_2019-07-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.35.2...v0.35.3)

This release exists only to rectify a publishing error that occurred with v0.35.2.

# v0.35.2 (_2019-07-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.35.1...v0.35.2)

This release exists only to rectify a publishing error that occurred with v0.35.1.

# v0.35.1 (_2019-07-24_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.35.0...v0.35.1)

## FxA Client

### What's Fixed

* Android: `migrateFromSessionToken` will not leave the account in a broken state if
  network errors happen during the migration process.

## Push

### What's Fixed

* Updated the default server host for the push service to match the production server.

# v0.35.0 (_2019-07-16_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.34.0...v0.35.0)

## General

### Megazords

The long-awaited android [megazord changes](./docs/design/megazords) have
arrived. This has a large number of changes, many of them breaking:
([#1103](https://github.com/mozilla/application-services/pull/1103))

- Consumers who depend on network features of application-services, but
  which were not using a megazord, will no longer be able to use a legacy
  HTTP stack by default.

- Consumers who depend on network features and *do* use a megazord, can no
  longer initialize HTTP in the same call as the megazord.

- Both of these cases should import the `org.mozilla.appservices:httpconfig`
  package, and call `RustHttpConfig.setClient(lazy { /* client to use */ })`
  before calling functions which make HTTP requests.

- For custom megazord users, the name of your megazord is now always
  `mozilla.appservices.Megazord`. You no longer need to load it by reflection,
  since the swapped-out version always has the same name as your custom version.

- The reference-browser megazord has effectively been replaced by the
  full-megazord, which is also the megazord used by default

- The steps to swap-out a custom megazord have changed. The specific steps are
  slightly different in various cases, and we will file PRs to help make the
  transition.

- Substitution builds once again work, except for running unit tests against
  Rust code.

## FxA Client

### What's Fixed

- The state persistence callback is now correctly triggered after a call
  to `FirefoxAccount.getProfile`.

### Breaking changes

- The `FirefoxAccount.destroyDevice` method has been removed in favor of the
  more general `FirefoxAccount.disconnect` method which will ensure a full
  disconnection by invalidating OAuth tokens and destroying the device record
  if it exists. ([#1397](https://github.com/mozilla/application-services/issues/1397))
- The `FirefoxAccount.disconnect` method has been added to the Swift bindings as well.
- The `FirefoxAccount.beginOAuthFlow` method will redirect to a content page that
  forces the user to connect to the last seen user email. To avoid this behavior,
  a new `FirefoxAccount` instance with a new persisted state must be created.

# v0.34.0 (_2019-07-10_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.33.2...v0.34.0)

## General

- All of our cryptographic primitives are now backed by NSS ([#1349](https://github.com/mozilla/application-services/pull/1349)). This change should be transparent our customers.

  If you build application-services, it is recommended to delete the `libs/{desktop, ios, android}` folders and start over using `./build-all.sh [android|desktop|ios]`. [GYP](https://github.com/mogemimi/pomdog/wiki/How-to-Install-GYP) and [ninja](https://github.com/ninja-build/ninja/wiki/Pre-built-Ninja-packages) are required to build these libraries.

## Places

### What's New

- Added `WritableHistoryConnection.acceptResult(searchString, url)` for marking
  an awesomebar result as accepted.
  ([#1332](https://github.com/mozilla/application-services/pull/1332))
    - Specifically, `queryAutocomplete` calls for searches that contain
      frequently accepted results are more highly ranked.

### Breaking changes

- Android only: The addition of `acceptResult` to `WritableHistoryConnection` is
  a breaking change for any custom implementations of `WritableHistoryConnection`
  ([#1332](https://github.com/mozilla/application-services/pull/1332))

## Push

### Breaking Changes

- `OpenSSLError` has been renamed to the more general `CryptoError`. ([#1349](https://github.com/mozilla/application-services/pull/1349))

# v0.33.2 (_2019-07-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.33.1...v0.33.2)

This release exists only to rectify a publishing error that occurred with v0.33.1.

# v0.33.1 (_2019-07-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.33.0...v0.33.1)

This release exists only to rectify a publishing error that occurred with v0.33.0.

# v0.33.0 (_2019-07-04_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.32.3...v0.33.0)

## FxA Client

### Breaking Changes

- iOS: FirefoxAccountError enum variants have their name `lowerCamelCased`
  instead of `UpperCamelCased`, to better fit with common Swift code style.
  ([#1324](https://github.com/mozilla/application-services/issues/1324))

# v0.32.3 (_2019-07-02_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.32.2...v0.32.3)

## Places

### What's Fixed

- `PlacesReaderConnection.queryAutocomplete` should return unique results. ([#970](https://github.com/mozilla/application-services/issues/970))

- Ensures bookmark sync doesn't fail if a bookmark or query is missing or has an invalid URL. ([#1325](https://github.com/mozilla/application-services/issues/1325))

# v0.32.2 (_2019-06-28_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.32.1...v0.32.2)

## General

- This is a release that aims to test infrastructure changes (ci-admin).

- OpenSSL dependency updated. ([#1328](https://github.com/mozilla/application-services/pull/1328))

# v0.32.1 (_2019-06-26_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.32.0...v0.32.1)

## FxA Client

### What's Fixed

- Fixes SendTab initializeDevice in Android to use the proper device type ([#1314](https://github.com/mozilla/application-services/pull/1314))

## iOS Bindings

### What's Fixed

- Errors emitted from the rust code should now all properly output their description. ([#1323](https://github.com/mozilla/application-services/pull/1323))

## Logins

### What's Fixed

- Remote login records which cannot be parsed are now ignored (and reported in telemetry). [#1253](https://github.com/mozilla/application-services/issues/1253)

# v0.32.0 (_2019-06-14_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.31.2...v0.32.0)

## Places

### What's fixed

- Fix an error that could happen when the place database is closed.
  ([#1304](https://github.com/mozilla/application-services/pull/1304))

- iOS only: Ensure interruption errors don't come through as network errors.
  ([#1304](https://github.com/mozilla/application-services/pull/1304))

# v0.31.3 (_2019-07-02_)

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.31.2...v0.31.3)

## General

- (Backport) Update `smallvec` dependency to pick up a security fix ([#1353](https://github.com/mozilla/application-services/pull/1353))

## Places

- (Backport) Ensures bookmark sync doesn't fail if a bookmark or query is missing or has an invalid URL ([#1325](https://github.com/mozilla/application-services/issues/1325))

## FxA Client

- (Backport) Fixes SendTab initializeDevice in Android to use the proper device type ([#1314](https://github.com/mozilla/application-services/pull/1314))

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
    - `fun getVisitInfos(start: Long, end: Long = Long.MAX_VALUE): List<VisitInfo>`: Returns a more detailed set of information about the visits that occurred. ([#619](https://github.com/mozilla/application-services/issues/619))
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

  This should be mostly transparent, but is a semi-breaking semantic change in the case that something throws an exception indicating that the Rust code panicked (which should only occur due to bugs anyway). If this occurs, all subsequent operations on that object (except `close`/`lock`) will cause errors. It is "poisoned", in Rust terminology. (In the future, this may be handled automatically)

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
- The fxa-client Android and iOS libraries will throw a differentiated exception for general network errors. ([#535](https://github.com/mozilla/application-services/pull/535))

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
