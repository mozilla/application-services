/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Firefox Accounts Client
//!
//! The fxa-client component lets applications integrate with the
//! [Firefox Accounts](https://mozilla.github.io/ecosystem-platform/docs/features/firefox-accounts/fxa-overview)
//! identity service. The shape of a typical integration would look
//! something like:
//!
//! * Out-of-band, register your application with the Firefox Accounts service,
//!   providing an OAuth `redirect_uri` controlled by your application and
//!   obtaining an OAuth `client_id`.
//!
//! * On application startup, create a [`FirefoxAccount`] object to represent the
//!   signed-in state of the application.
//!     * On first startup, a new [`FirefoxAccount`] can be created by calling
//!       [`FirefoxAccount::new`] and passing the application's `client_id`.
//!     * For subsequent startups the object can be persisted using the
//!       [`to_json`](FirefoxAccount::to_json) method and re-created by
//!       calling [`FirefoxAccount::from_json`].
//!
//! * When the user wants to sign in to your application, direct them through
//!   a web-based OAuth flow using [`begin_oauth_flow`](FirefoxAccount::begin_oauth_flow)
//!   or [`begin_pairing_flow`](FirefoxAccount::begin_pairing_flow); when they return
//!   to your registered `redirect_uri`, pass the resulting authorization state back to
//!   [`complete_oauth_flow`](FirefoxAccount::complete_oauth_flow) to sign them in.
//!
//! * Display information about the signed-in user by using the data from
//!   [`get_profile`](FirefoxAccount::get_profile).
//!
//! * Access account-related services on behalf of the user by obtaining OAuth
//!   access tokens via [`get_access_token`](FirefoxAccount::get_access_token).
//!
//! * If the user opts to sign out of the application, calling [`disconnect`](FirefoxAccount::disconnect)
//!   and then discarding any persisted account data.

mod account;
mod auth;
mod device;
mod error;
mod events;
mod migration;
mod profile;
mod push;
mod storage;
mod telemetry;
mod token;

pub use auth::{MetricsParams, OAuthResult};
pub use device::{AttachedClient, Device, DeviceCapability, DeviceRecord};
pub use error::{Error, FxaError};
pub use events::FxaEventHandler;
pub use migration::{FxAMigrationResult, MigrationState};
pub use profile::Profile;
pub use push::{
    AccountEvent, DevicePushSubscription, IncomingDeviceCommand, ParsedPushMessage, PushMessageDisplay, SendTabPayload, TabHistoryEntry,
    parse_push_message,
};
pub use storage::{FxaStorage, SavedState};
pub use token::{AccessTokenInfo, AuthorizationParameters, ScopedKey};

// All the implementation details live in this "internal" module.
// Aspirationally, I'd like to make it private, so that the public API of the crate
// is entirely the same as the API exposed to consumers via UniFFI. That's not
// possible right now because some of our tests/example use features that we do
// not currently expose to consumers. But we should figure out how to expose them!
pub mod internal;

/// Result returned by internal functions
type Result<T> = std::result::Result<T, Error>;
/// Result returned by public-facing API functions
type ApiResult<T> = std::result::Result<T, FxaError>;

/// Object representing the signed-in state of an application.
///
/// The `FirefoxAccount` object is the main interface provided by this crate.
/// It represents the signed-in state of an application that may be connected to
/// user's Firefox Account, and provides methods for inspecting the state of the
/// account and accessing other services on behalf of the user.
///
pub struct FirefoxAccount {
    // For now, we serialize all access on a single `Mutex` for thread safety across
    // the FFI. We should make the locking more granular in future.
    internal: std::sync::Mutex<internal::FirefoxAccount>,
}

impl FirefoxAccount {
    /// Create a new [`FirefoxAccount`] instance, not connected to any account.
    ///
    /// This method constructs as new [`FirefoxAccount`] instance configured to connect
    /// the application to a user's account.
    ///
    /// A async call to the `FxaStorage::load_state` will be queued to restore the state from
    /// before the last shutdown.  The [`FirefoxAccount`] instance will be in the
    /// [FxaState::Loading] state until then.
    ///
    /// Note: because `load_state` runs asynchronously, constructing a `FirefoxAccount` does not
    /// block on any IO.  This means it's safe to construct a `FirefoxAccount` instance early in
    /// the startup process.  If applications want to delay loading from disk to speed up the
    /// startup process, they should should add that delay to `load_state()` rather than waiting to
    /// construct the `FirefoxAccount` instance.
    pub fn new(config: FxaConfig, storage: Box<dyn FxaStorage>, event_handlers: Vec<Box<dyn FxaEventHandler>>) -> FirefoxAccount {
        unimplemented!()
    }

    /// Get the current state of the client
    pub fn get_state(&self) -> FxaStateInfo {
        unimplemented!()
    }
}

pub enum FxaServer {
    Release,
    Stable,
    Stage,
    China,
    LocalDev,
}

pub struct FxaConfig {
    /// FxaServer to connect with
    pub server: FxaServer,
    /// registered OAuth client id of the application.
    pub client_id: String,
    /// `redirect_uri` - the registered OAuth redirect URI of the application.
    pub redirect_uri: String,
    ///  URL for the user's Sync Tokenserver. This can be used to support users who self-host their
    ///  sync data. If `None` then it will default to the Mozilla-hosted Sync server.
    pub token_server_url_override: Option<String>,
    /// Device record to register with the FxA server
    pub device_record: DeviceRecord,
}

pub struct FxaStateInfo {
    /// The current state of the FxA client
    pub state: FxaState,
    /// Are we in the middle of a transition to another state?
    ///
    /// If this is not None, state-changing methods like [FirefoxAccount::begin_oauth_flow()] or
    /// [FirefoxAccount::disconnect()] will fail with an [FxaError::InvalidState] error.
    pub transition: Option<FxaStateTransition>,
}

pub enum FxaState {
    /// User is logged out
    Disconnected,
    /// User is logged in
    Connected,
    /// User has been logged out by some external event and needs to re-authenticate, for example a
    /// password change from another device
    ReauthenticationNeeded,
}

pub enum FxaStateTransition {
    /// User is currently going through an OAuth flow
    Authenticating,
    /// The client is waiting for the saved state to load.
    Loading,
}
