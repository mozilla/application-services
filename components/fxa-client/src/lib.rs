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

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
use serde_derive::*;
use std::collections::HashMap;
use thiserror::Error;

// All the implementation details live in this "internal" module.
// Aspirationally, I'd like to make it private, so that the public API of the crate
// is entirely the same as the API exposed to consumers via UniFFI. That's not
// possible right now because some of our tests/example use features that we do
// not currently expose to consumers. But we should figure out how to expose them!
pub mod internal;

uniffi_macros::include_scaffolding!("fxa_client");

/// Generic error type thrown by many [`FirefoxAccount`] operations.
///
/// Precise details of the error are hidden from consumers, mostly due to limitations of
/// how we expose this API to other languages. The type of the error indicates how the
/// calling code should respond.
///
#[derive(Debug, Error)]
pub enum FxaError {
    /// Thrown when there was a problem with the authentication status of the account,
    /// such as an expired token. The application should [check its authorization status](
    /// FirefoxAccount::check_authorization_status) to see whether it has been disconnected,
    /// or retry the operation with a freshly-generated token.
    #[error("authentication error")]
    Authentication,
    /// Thrown if an operation fails due to network access problems.
    /// The application may retry at a later time once connectivity is restored.
    #[error("network error")]
    Network,
    /// Thrown if the application attempts to complete an OAuth flow when no OAuth flow
    /// has been initiated. This may indicate a user who navigated directly to the OAuth
    /// `redirect_uri` for the application.
    ///
    /// **Note:** This error is currently only thrown in the Swift language bindings.
    #[error("no authentication flow was active")]
    NoExistingAuthFlow,
    /// Thrown if the application attempts to complete an OAuth flow, but the state
    /// tokens returned from the Firefox Account server do not match with the ones
    /// expected by the client.
    /// This may indicate a stale OAuth flow, or potentially an attempted hijacking
    /// of the flow by an attacker. The signin attempt cannot be completed.
    ///
    /// **Note:** This error is currently only thrown in the Swift language bindings.
    #[error("the requested authentication flow was not active")]
    WrongAuthFlow,
    /// Thrown if there is a panic in the underlying Rust code.
    ///
    /// **Note:** This error is currently only thrown in the Kotlin language bindings.
    #[error("panic in native code")]
    Panic,
    /// A catch-all for other unspecified errors.
    #[error("other error")]
    Other,
}

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

/// # Constructors and state management
///
/// These are methods for managing the signed-in state of the application,
/// either by restoring a previously-saved state via [`FirefoxAccount::from_json`]
/// or by starting afresh with [`FirefoxAccount::new`].
///
/// The application must persist the signed-in state after calling any methods
/// that may alter it. Such methods are marked in the documentation as follows:
///
/// **💾 This method alters the persisted account state.**
///
/// After calling any such method, use [`FirefoxAccount::to_json`] to serialize
/// the modified account state and persist the resulting string in application
/// settings.
///
impl FirefoxAccount {
    /// Create a new [`FirefoxAccount`] instance, not connected to any account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method constructs as new [`FirefoxAccount`] instance configured to connect
    /// the application to a user's account.
    ///
    /// # Arguments
    ///
    ///   - `content_url` - the URL of the Firefox Accounts server to use
    ///       - For example, use `https://accounts.firefox.com` for the main
    ///         Mozilla-hosted service.
    ///   - `client_id` - the registered OAuth client id of the application.
    ///   - `redirect_uri` - the registered OAuth redirect URI of the application.
    ///   - `token_server_url_override`: optionally, URL for the user's Sync Tokenserver.
    ///        - This can be used to support users who self-host their sync data.
    ///          If `None` then it will default to the Mozilla-hosted Sync server.
    ///
    pub fn new(
        content_url: &str,
        client_id: &str,
        redirect_uri: &str,
        token_server_url_override: &Option<String>,
    ) -> FirefoxAccount {
        FirefoxAccount {
            internal: std::sync::Mutex::new(internal::FirefoxAccount::new(
                content_url,
                client_id,
                redirect_uri,
                token_server_url_override.as_deref(),
            )),
        }
    }

    /// Restore a [`FirefoxAccount`] instance from serialized state.
    ///
    /// Given a JSON string previously obtained from [`FirefoxAccount::to_json`], this
    /// method will deserialize it and return a live [`FirefoxAccount`] instance.
    ///
    /// **⚠️ Warning:** since the serialized state contains access tokens, you should
    /// not call `from_json` multiple times on the same data. This would result
    /// in multiple live objects sharing the same access tokens and is likely to
    /// produce unexpected behaviour.
    ///
    pub fn from_json(data: &str) -> Result<FirefoxAccount, FxaError> {
        Ok(FirefoxAccount {
            internal: std::sync::Mutex::new(internal::FirefoxAccount::from_json(data)?),
        })
    }

    /// Save current state to a JSON string.
    ///
    /// This method serializes the current account state into a JSON string, which
    /// the application can use to persist the user's signed-in state across restarts.
    /// The application should call this method and update its persisted state after
    /// any potentially-state-changing operation.
    ///
    /// **⚠️ Warning:** the serialized state may contain encryption keys and access
    /// tokens that let anyone holding them access the user's data in Firefox Sync
    /// and/or other FxA services. Applications should take care to store the resulting
    /// data in a secure fashion, as appropriate for their target platform.
    ///
    pub fn to_json(&self) -> Result<String, FxaError> {
        Ok(self.internal.lock().unwrap().to_json()?)
    }
}

/// # Signing in and out
///
/// These are methods for managing the signed-in state, such as authenticating via
/// an OAuth flow or disconnecting from the user's account.
///
/// The Firefox Accounts system supports two methods for connecting an application
/// to a user's account:
///
///    - A traditional OAuth flow, where the user is directed to a webpage to enter
///      their account credentials and then redirected back to the application.
///      This is exposed by the [`begin_oauth_flow`](FirefoxAccount::begin_oauth_flow)
///      method.
///
///    - A device pairing flow, where the user scans a QRCode presented by another
///      app that is already connected to the account, which then directs them to
///      a webpage for a simplified signing flow. This is exposed by the
///      [`begin_pairing_flow`](FirefoxAccount::begin_pairing_flow) method.
///
/// Technical details of the pairing flow can be found in the [Firefox Accounts
/// documentation hub](https://mozilla.github.io/ecosystem-platform/docs/features/firefox-accounts/pairing).
///
impl FirefoxAccount {
    /// Initiate a web-based OAuth sign-in flow.
    ///
    /// This method initializes some internal state and then returns a URL at which the
    /// user may perform a web-based authorization flow to connect the application to
    /// their account. The application should direct the user to the provided URL.
    ///
    /// When the resulting OAuth flow redirects back to the configured `redirect_uri`,
    /// the query parameters should be extracting from the URL and passed to the
    /// [`complete_oauth_flow`](FirefoxAccount::complete_oauth_flow) method to finalize
    /// the signin.
    ///
    /// # Arguments
    ///
    ///   - `scopes` - list of OAuth scopes to request.
    ///       - The requested scopes will determine what account-related data
    ///         the application is able to access.
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user triggered the signin request.
    ///         For example, the application toolbar, on the onboarding flow.
    ///   - `metrics` - optionally, additional metrics tracking paramters.
    ///       - These will be included as query parameters in the resulting URL.
    ///
    pub fn begin_oauth_flow(
        &self,
        scopes: &[String],
        entrypoint: &str,
        metrics: Option<MetricsParams>,
    ) -> Result<String, FxaError> {
        // UniFFI can't represent `&[&str]` yet, so convert it internally here.
        let scopes = scopes.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(self
            .internal
            .lock()
            .unwrap()
            .begin_oauth_flow(&scopes, entrypoint, metrics)?)
    }

    /// Get the URL at which to begin a device-pairing signin flow.
    ///
    /// If the user wants to sign in using device pairing, call this method and then
    /// direct them to visit the resulting URL on an already-signed-in device. Doing
    /// so will trigger the other device to show a QR code to be scanned, and the result
    /// from said QR code can be passed to [`begin_pairing_flow`](FirefoxAccount::begin_pairing_flow).
    ///
    pub fn get_pairing_authority_url(&self) -> Result<String, FxaError> {
        Ok(self.internal.lock().unwrap().get_pairing_authority_url()?)
    }

    /// Initiate a device-pairing sign-in flow.
    ///
    /// Once the user has scanned a pairing QR code, pass the scanned value to this
    /// method. It will return a URL to which the application should redirect the user
    /// in order to continue the sign-in flow.
    ///
    /// When the resulting flow redirects back to the configured `redirect_uri`,
    /// the resulting OAuth parameters should be extracting from the URL and passed
    /// to [`complete_oauth_flow`](FirefoxAccount::complete_oauth_flow) to finalize
    /// the signin.
    ///
    /// # Arguments
    ///
    ///   - `pairing_url` - the URL scanned from a QR code on another device.
    ///   - `scopes` - list of OAuth scopes to request.
    ///       - The requested scopes will determine what account-related data
    ///         the application is able to access.
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user triggered the signin request.
    ///         For example, the application toolbar, on the onboarding flow.
    ///   - `metrics` - optionally, additional metrics tracking paramters.
    ///       - These will be included as query parameters in the resulting URL.
    ///
    pub fn begin_pairing_flow(
        &self,
        pairing_url: &str,
        scopes: &[String],
        entrypoint: &str,
        metrics: Option<MetricsParams>,
    ) -> Result<String, FxaError> {
        // UniFFI can't represent `&[&str]` yet, so convert it internally here.
        let scopes = scopes.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(self.internal.lock().unwrap().begin_pairing_flow(
            pairing_url,
            &scopes,
            entrypoint,
            metrics,
        )?)
    }

    /// Complete an OAuth flow.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// At the conclusion of an OAuth flow, the user will be redirect to the
    /// application's registered `redirect_uri`. It should extract the `code`
    /// and `state` parameters from the resulting URL and pass them to this
    /// method in order to complete the sign-in.
    ///
    /// # Arguments
    ///
    ///   - `code` - the OAuth authorization code obtained from the redirect URI.
    ///   - `state` - the OAuth state parameter obtained from the redirect URI.
    ///
    pub fn complete_oauth_flow(&self, code: &str, state: &str) -> Result<(), FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .complete_oauth_flow(code, state)?)
    }

    /// Check authorization status for this application.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications may call this method to check with the FxA server about the status
    /// of their authentication tokens. It returns an [`AuthorizationInfo`] struct
    /// with details about whether the tokens are still active.
    ///
    pub fn check_authorization_status(&self) -> Result<AuthorizationInfo, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .check_authorization_status()?
            .into())
    }

    /// Disconnect from the user's account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method destroys any tokens held by the client, effectively disconnecting
    /// from the user's account. Applications should call this when the user opts to
    /// sign out.
    ///
    /// The persisted account state after calling this method will contain only the
    /// user's last-seen profile information, if any. This may be useful in helping
    /// the user to reconnnect to their account. If reconnecting to the same account
    /// is not desired then the application should discard the persisted account state.
    ///
    pub fn disconnect(&self) {
        self.internal.lock().unwrap().disconnect()
    }
}

/// # User Profile info
///
/// These methods can be used to find out information about the connected user.
///
impl FirefoxAccount {
    /// Get profile information for the signed-in user, if any.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method fetches a [`Profile`] struct with information about the currently-signed-in
    /// user, either by using locally-cached profile information or by fetching fresh data from
    /// the server.
    ///
    /// # Arguments
    ///
    ///    - `ignore_cache` - if true, always hit the server for fresh profile information.
    ///
    /// # Notes
    ///
    ///    - Profile information is only available to applications that have been
    ///      granted the `profile` scope.
    ///    - There is currently no API for fetching cached profile information without
    ///      potentially hitting the server.
    ///    - If there is no signed-in user, this method will throw an
    ///      [`Authentication`](FxaError::Authentication) error.
    ///
    pub fn get_profile(&self, ignore_cache: bool) -> Result<Profile, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .get_profile(ignore_cache)?
            .into())
    }
}

/// # Device Management
///
/// Applications that connect to a user's account may register additional information
/// about themselves via a "device record", which allows them to:
///
///    - customize how they appear in the user's account management page
///    - receive push notifications about events that happen on the account
///    - participate in the FxA "device commands" ecosystem
///
/// For more details on FxA device registration and management, consult the
/// [Firefox Accounts Device Registration docs](
/// https://github.com/mozilla/fxa/blob/main/packages/fxa-auth-server/docs/device_registration.md).
///
impl FirefoxAccount {
    /// Create a new device record for this application.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method registed a device record for the application, providing basic metadata for
    /// the device along with a list of supported [Device Capabilities](DeviceCapability) for
    /// participating in the "device commands" ecosystem.
    ///
    /// Applications should call this method soon after a successful sign-in, to ensure
    /// they they appear correctly in the user's account-management pages and when discovered
    /// by other devices connected to the account.
    ///
    /// # Arguments
    ///
    ///    - `name` - human-readable display name to use for this application
    ///    - `device_type` - the [type](DeviceType) of device the application is installed on
    ///    - `supported_capabilities` - the set of [capabilities](DeviceCapability) to register
    ///       for this device in the "device commands" ecosystem.
    ///
    /// # Notes
    ///
    ///    - Device registration is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn initialize_device(
        &self,
        name: &str,
        device_type: DeviceType,
        supported_capabilities: Vec<DeviceCapability>,
    ) -> Result<(), FxaError> {
        // UniFFI doesn't have good handling of lists of references, work around it.
        let supported_capabilities: Vec<_> =
            supported_capabilities.into_iter().map(Into::into).collect();
        Ok(self.internal.lock().unwrap().initialize_device(
            name,
            device_type.into(),
            &supported_capabilities,
        )?)
    }

    /// Get the device id registered for this application.
    ///
    /// # Notes
    ///
    ///    - If the application has not registered a device record, this method will
    ///      throw an [`Other`](FxaError::Other) error.
    ///        - (Yeah...sorry. This should be changed to do something better.)
    ///    - Device metadata is only visible to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn get_current_device_id(&self) -> Result<String, FxaError> {
        Ok(self.internal.lock().unwrap().get_current_device_id()?)
    }

    /// Get the list of devices registered on the user's account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method returns a list of [`Device`] structs representing all the devices
    /// currently attached to the user's account (including the current device).
    /// The application might use this information to e.g. display a list of appropriate
    /// send-tab targets.
    ///
    /// # Arguments
    ///
    ///    - `ignore_cache` - if true, always hit the server for fresh profile information.
    ///
    /// # Notes
    ///
    ///    - Device metadata is only visible to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn get_devices(&self, ignore_cache: bool) -> Result<Vec<Device>, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .get_devices(ignore_cache)?
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?)
    }

    /// Get the list of all client applications attached to the user's account.
    ///
    /// This method returns a list of [`AttachedClient`] structs representing all the applications
    /// connected to the user's acount. This includes applications that are registered as a device
    /// as well as server-side services that the user has connected.
    ///
    /// This information is really only useful for targetted messaging or marketing purposes,
    /// e.g. if the application wants to advertize a related product, but first wants to check
    /// whether the user is already using that product.
    ///
    /// # Notes
    ///
    ///    - Attached client metadata is only visible to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn get_attached_clients(&self) -> Result<Vec<AttachedClient>, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .get_attached_clients()?
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?)
    }

    /// Update the display name used for this application instance.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method modifies the name of the current application's device record, as seen by
    /// other applications and in the user's account management pages.
    ///
    /// # Arguments
    ///
    ///    - `display_name` - the new name for the current device.
    ///
    /// # Notes
    ///
    ///    - Device registration is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn set_device_name(&self, display_name: &str) -> Result<(), FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .set_device_name(display_name)?)
    }

    /// Clear any custom display name used for this application instance.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method clears the name of the current application's device record, causing other
    /// applications or the user's account management pages to have to fill in some sort of
    /// default name when displaying this device.
    ///
    /// # Notes
    ///
    ///    - Device registration is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn clear_device_name(&self) -> Result<(), FxaError> {
        Ok(self.internal.lock().unwrap().clear_device_name()?)
    }

    /// Ensure that the device record has a specific set of capabilities.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method checks that the currently-registred device record is advertising the
    /// given set of capabilities in the FxA "device commands" ecosystem. If not, then it
    /// updates the device record to do so.
    ///
    /// Applications should call this method on each startup as a way to ensure that their
    /// expected set of capabilities is being accurately reflected on the FxA server, and
    /// to handle the rollout of new capabilities over time.
    ///
    /// # Arguments
    ///
    ///    - `supported_capabilities` - the set of [capabilities](DeviceCapability) to register
    ///       for this device in the "device commands" ecosystem.
    ///
    /// # Notes
    ///
    ///    - Device registration is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn ensure_capabilities(
        &self,
        supported_capabilities: Vec<DeviceCapability>,
    ) -> Result<(), FxaError> {
        let supported_capabilities: Vec<_> =
            supported_capabilities.into_iter().map(Into::into).collect();
        Ok(self
            .internal
            .lock()
            .unwrap()
            .ensure_capabilities(&supported_capabilities)?)
    }

    /// Set or update a push subscription endpoint for this device.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method registers the given webpush subscription with the FxA server, requesting
    /// that is send notifications in the event of any significant changes to the user's
    /// account. When the application receives a push message at the registered subscription
    /// endpoint, it should decrypt the payload and pass it to the [`handle_push_message`](
    /// FirefoxAccount::handle_push_message) method for processing.
    ///
    /// # Arguments
    ///
    ///    - `subscription` - the [`DevicePushSubscription`] details to register with the server.
    ///
    /// # Notes
    ///
    ///    - Device registration is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn set_push_subscription(
        &self,
        subscription: DevicePushSubscription,
    ) -> Result<(), FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .set_push_subscription(subscription.into())?)
    }

    /// Process and respond to server-delivered account update messages.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications should call this method whenever they receive a push notiication on subscription
    /// endpoint previously registered with the Firefox Accounts server. Such messages typically indicate
    /// a noteworthy change of state on the user's account, such as an update to their profile information
    /// or the disconnection of a client. The [`FirefoxAccount`] struct will update its internl state
    /// accordingly and return a list of [`AccountEvent`] structs describing the events, which the application
    /// may use for further processing.
    ///
    pub fn handle_push_message(&self, payload: &str) -> Result<Vec<AccountEvent>, FxaError> {
        Ok(self.internal.lock().unwrap().handle_push_message(payload)?)
    }

    /// Poll the server for any pending device commands.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that have registered one or more [`DeviceCapability`]s with the server can use
    /// this method to check whether other devices on the account have sent them any commands.
    /// It will return a list of [`IncomingDeviceCommand`] structs for the application to process.
    ///
    /// # Notes
    ///
    ///    - Device commands are typically delivered via push message and the [`CommandReceived`](
    ///      AccountEvent::CommandReceived) event. Polling should only be used as a backup delivery
    ///      mechanism, f the application has reason to believe that push messages may have been missed.
    ///    - Device commands functionality is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn poll_device_commands(&self) -> Result<Vec<IncomingDeviceCommand>, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .poll_device_commands(internal::device::CommandFetchReason::Poll)?
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<_, _>>()?)
    }

    /// Use device commands to send a single tab to another device.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// If a device on the account has registered the [`SendTab`](DeviceCapability::SendTab)
    /// capability, this method can be used to send it a tab.
    ///
    /// # Notes
    ///
    ///    - If the given device id does not existing or is not capable of receiving tabs,
    ///      this method will throw an [`Other`](FxaError::Other) error.
    ///        - (Yeah...sorry. This should be changed to do something better.)
    ///    - It is not currently possible to send a full [`SendTabPayload`] to another device,
    ///      but that's purely an API limitation that should go away in future.
    ///    - Device commands functionality is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    ///
    pub fn send_single_tab(
        &self,
        target_device_id: &str,
        title: &str,
        url: &str,
    ) -> Result<(), FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .send_single_tab(target_device_id, title, url)?)
    }
}

/// # Account Management URLs
///
/// Signed-in applications should not attempt to perform an account-level management
/// (such as changing profile data or managing devices) using native UI. Instead, they
/// should offer the user the opportunity to visit their account managment pages on the
/// web.
///
/// The methods in this section provide URLs at which the user can perform various
/// account-management activities.
///
impl FirefoxAccount {
    /// Get the URL at which to access the user's sync data.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    pub fn get_token_server_endpoint_url(&self) -> Result<String, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .get_token_server_endpoint_url()?)
    }

    /// Get a URL which shows a "successfully connceted!" message.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications can use this method after a successful signin, to redirect the
    /// user to a success message displayed in web content rather than having to
    /// implement their own native success UI.
    ///
    pub fn get_connection_success_url(&self) -> Result<String, FxaError> {
        Ok(self.internal.lock().unwrap().get_connection_success_url()?)
    }

    /// Get a URL at which the user can manage their account and profile data.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications should link the user out to this URL from an appropriate place
    /// in their signed-in settings UI.
    ///
    /// # Arguments
    ///
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user followed the link.
    ///
    pub fn get_manage_account_url(&self, entrypoint: &str) -> Result<String, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .get_manage_account_url(entrypoint)?)
    }

    /// Get a URL at which the user can manage the devices connected to their account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications should link the user out to this URL from an appropriate place
    /// in their signed-in settings UI. For example, "Manage your devices..." may be
    /// a useful link to place somewhere near the device list in the send-tab UI.
    ///
    /// # Arguments
    ///
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user followed the link.
    ///
    pub fn get_manage_devices_url(&self, entrypoint: &str) -> Result<String, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .get_manage_devices_url(entrypoint)?)
    }
}

/// # Token Management
///
/// A signed-in appliction will typically hold a number of different *tokens* associated with the
/// user's account, including:
///
///    - An OAuth `refresh_token`, representing their ongoing connection to the account
///      and the scopes that have been granted.
///    - Short-lived OAuth `access_token`s that can be used to access resources on behalf
///      of the user.
///    - Optionally, a `session_token` that gives full control over the user's account,
///      typically managed on behalf of web content that runs within the context
///      of the application.
///
impl FirefoxAccount {
    /// Get a short-lived OAuth access token for the user's account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that need to access resources on behalf of the user must obtain an
    /// `access_token` in order to do so. For example, an access token is required when
    /// fetching the user's profile data, or when accessing their data stored in Firefox Sync.
    ///
    /// This method will obtain and return an access token bearing the requested scopes, either
    /// from a local cache of previously-issued tokens, or by creating a new one from the server.
    ///
    /// # Arguments
    ///
    ///    - `scope` - the OAuth scope to be granted by the token.
    ///        - This must be one of the scopes requested during the signin flow.
    ///        - Only a single scope is supported; for multiple scopes request multiple tokens.
    ///    - `ttl` - optionally, the time for which the token should be valid, in seconds.
    ///
    /// # Notes
    ///
    ///    - If the application receives an authorization error when trying to use the resulting
    ///      token, it should call [`clear_access_token_cache`](FirefoxAccount::clear_access_token_cache)
    ///      before requesting a fresh token.
    ///
    pub fn get_access_token(
        &self,
        scope: &str,
        ttl: Option<i64>,
    ) -> Result<AccessTokenInfo, FxaError> {
        // Signedness converstion for Kotlin compatibility :-/
        let ttl = ttl.map(|ttl| u64::try_from(ttl).unwrap_or_default());
        Ok(self
            .internal
            .lock()
            .unwrap()
            .get_access_token(scope, ttl)?
            .try_into()?)
    }

    /// Get the session token for the user's account, if one is available.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that function as a web browser may need to hold on to a session token
    /// on behalf of Firefox Accounts web content. This method exists so that they can retreive
    /// it an pass it back to said web content when required.
    ///
    /// # Notes
    ///
    ///    - Please do not attempt to use the resulting token to directly make calls to the
    ///      Firefox Accounts servers! All account management functionality should be performed
    ///      in web content.
    ///    - A session token is only available to applications that have requested the
    ///      `https://identity.mozilla.com/tokens/session` scope.
    ///
    pub fn get_session_token(&self) -> Result<String, FxaError> {
        Ok(self.internal.lock().unwrap().get_session_token()?)
    }

    /// Update the stored session token for the user's account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that function as a web browser may need to hold on to a session token
    /// on behalf of Firefox Accounts web content. This method exists so that said web content
    /// signals that it has generated a new session token, the stored value can be updated
    /// to match.
    ///
    /// # Arguments
    ///
    ///    - `session_token` - the new session token value provided from web content.
    ///
    pub fn handle_session_token_change(&self, session_token: &str) -> Result<(), FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .handle_session_token_change(session_token)?)
    }

    /// Create a new OAuth authorization code using the stored session token.
    ///
    /// When a signed-in application receives an incoming device pairing request, it can
    /// use this method to grant the request and generate a corresponding OAuth authorization
    /// code. This code would then be passed back to the connecting device over the
    /// pairing channel (a process which is not currently supported by any code in this
    /// component).
    ///
    /// # Arguments
    ///
    ///    - `params` - the OAuth parameters from the incoming authorization request
    ///
    pub fn authorize_code_using_session_token(
        &self,
        params: AuthorizationParameters,
    ) -> Result<String, FxaError> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .authorize_code_using_session_token(params)?)
    }

    /// Clear the access token cache in response to an auth failure.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that receive an authentication error when trying to use an access token,
    /// should call this method before creating a new token and retrying the failed operation.
    /// It ensures that the expired token is removed and a fresh one generated.
    ///
    pub fn clear_access_token_cache(&self) {
        self.internal.lock().unwrap().clear_access_token_cache()
    }
}

/// # Telemetry Methods
///
/// This component does not currently submit telemetry via Glean, but it *does* gather
/// a small amount of telemetry about send-tab that the application may submit on its
/// behalf.
///
impl FirefoxAccount {
    /// Collect and return telemetry about send-tab attempts.
    ///
    /// Applications that register the [`SendTab`](DeviceCapability::SendTab) capability
    /// should also arrange to submit "sync ping" telemetry. Calling this method will
    /// return a JSON string of telemetry data that can be incorporated into that ping.
    ///
    /// Sorry, this is not particularly carefully documented because it is intended
    /// as a stop-gap until we get native Glean support. If you know how to submit
    /// a sync ping, you'll know what to do with the contents of the JSON string.
    ///
    pub fn gather_telemetry(&self) -> Result<String, FxaError> {
        Ok(self.internal.lock().unwrap().gather_telemetry()?)
    }
}

/// # Migration Support Methods
///
/// Some applications may have existing signed-in account state from a bespoke implementation
/// of the Firefox Accounts signin protocol, but want to move to using this component in order
/// to reduce maintenance costs.
///
/// The sign-in state for a legacy FxA integration would typically consist of a session token
/// and a pair of cryptographic keys used for accessing Firefox Sync. The methods in this section
/// can be used to help migrate from such legacy state into state that's suitable for use with
/// this component.
///
impl FirefoxAccount {
    /// Sign in by using legacy session-token state.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// When migrating to use the FxA client component, create a [`FirefoxAccount`] instance
    /// and then pass any legacy sign-in state to this method. It will attempt to use the
    /// session token to bootstrap a full internal state of OAuth tokens, and will store the
    /// provided credentials internally in case it needs to retry after e.g. a network failure.
    ///
    /// # Arguments
    ///
    ///    - `session_token` - the session token from legacy sign-in state
    ///    - `k_sync` - the Firefox Sync encryption key from legacy sign-in state
    ///    - `k_xcs` - the Firefox Sync "X-Client-State: value from legacy sign-in state
    ///    - `copy_session_token` - if true, copy the given session token rather than using it directly
    ///
    /// # Notes
    ///
    ///    - If successful, this method will return an [`FxAMigrationResult`] with some statistics
    ///      about the migration process.
    ///    - If unsuccessful this method will throw an error, but you may be able to retry the
    ///      migration again at a later time.
    ///    - Use [is_in_migration_state](FirefoxAccount::is_in_migration_state) to check whether the
    ///      persisted account state includes a a pending migration that can be retried.
    ///
    pub fn migrate_from_session_token(
        &self,
        session_token: &str,
        k_sync: &str,
        k_xcs: &str,
        copy_session_token: bool,
    ) -> Result<FxAMigrationResult, FxaError> {
        Ok(self.internal.lock().unwrap().migrate_from_session_token(
            session_token,
            k_sync,
            k_xcs,
            copy_session_token,
        )?)
    }

    /// Retry a previously failed migration from legacy session-token state.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// If an earlier call to [`migrate_from_session_token`](FirefoxAccount::migrate_from_session_token)
    /// failed, it may have stored the provided state for retrying at a later time. Call this method
    /// in order to execute such a retry.
    ///
    pub fn retry_migrate_from_session_token(&self) -> Result<FxAMigrationResult, FxaError> {
        Ok(self.internal.lock().unwrap().try_migration()?)
    }

    /// Check for a previously failed migration from legacy session-token state.
    ///
    /// If an earlier call to [`migrate_from_session_token`](FirefoxAccount::migrate_from_session_token)
    /// failed, it may have stored the provided state for retrying at a later time. Call this method
    /// in check whether such state exists, then retry at an appropriate time.
    ///
    pub fn is_in_migration_state(&self) -> MigrationState {
        self.internal.lock().unwrap().is_in_migration_state()
    }
}

/// Information about the authorization state of the application.
///
/// This struct represents metadata about whether the application is currently
/// connected to the user's account.
///
pub struct AuthorizationInfo {
    pub active: bool,
}

/// Additional metrics tracking parameters to include in an OAuth request.
///
pub struct MetricsParams {
    pub parameters: HashMap<String, String>,
}

/// An OAuth access token, with its associated keys and metadata.
///
/// This struct represents an FxA OAuth access token, which can be used to access a resource
/// or service on behalf of the user. For example, accessing the user's data in Firefox Sync
/// an access token for the scope `https://identity.mozilla.com/apps/sync` along with the
/// associated encryption key.
///
pub struct AccessTokenInfo {
    /// The scope of access granted by token.
    pub scope: String,
    /// The access token itself.
    ///
    /// This is the value that should be included in the `Authorization` header when
    /// accessing an OAuth protected resource on behalf of the user.
    pub token: String,
    /// The client-side encryption key associated with this scope.
    ///
    /// **⚠️ Warning:** the value of this field should never be revealed outside of the
    /// application. For example, it should never to sent to a server or logged in a log file.
    pub key: Option<ScopedKey>,
    /// The expiry time of the token, in seconds.
    ///
    /// This is the timestamp at which the token is set to expire, in seconds since
    /// unix epoch. Note that it is a signed integer, for compatibility with languages
    /// that do not have an unsigned integer type.
    ///
    /// This timestamp is for guidance only. Access tokens are not guaranteed to remain
    /// value for any particular lengthof time, and consumers should be prepared to handle
    /// auth failures even if the token has not yet expired.
    pub expires_at: i64,
}

/// A cryptograpic key associated with an OAuth scope.
///
/// Some OAuth scopes have a corresponding client-side encryption key that is required
/// in order to access protected data. This struct represents such key material in a
/// format compatible with the common "JWK" standard.
///
#[derive(Clone, Serialize, Deserialize)]
pub struct ScopedKey {
    /// The type of key.
    ///
    /// In practice for FxA, this will always be string string "oct" (short for "octal")
    /// to represent a raw symmetric key.
    pub kty: String,
    /// The OAuth scope with which this key is associated.
    pub scope: String,
    /// The key material, as base64-url-encoded bytes.
    ///
    /// **⚠️ Warning:** the value of this field should never be revealed outside of the
    /// application. For example, it should never to sent to a server or logged in a log file.
    pub k: String,
    /// An opaque unique identifier for this key.
    ///
    /// Unlike the `k` field, this value is not secret and may be revealed to the server.
    pub kid: String,
}

/// Parameters provided in an incoming OAuth request.
///
/// This struct represents parameters obtained from an incoming OAuth request - that is,
/// the values that an OAuth client would append to the authorization URL when initiating
/// an OAuth sign-in flow.
///
pub struct AuthorizationParameters {
    pub client_id: String,
    pub scope: Vec<String>,
    pub state: String,
    pub access_type: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub keys_jwk: Option<String>,
}

/// A device connected to the user's account.
///
/// This struct provides metadata about a device connected to the user's account.
/// This data would typically be used to display e.g. the list of candidate devices
/// in a "send tab" menu.
///
#[derive(Debug)]
pub struct Device {
    pub id: String,
    pub display_name: String,
    pub device_type: DeviceType,
    pub capabilities: Vec<DeviceCapability>,
    pub push_subscription: Option<DevicePushSubscription>,
    pub push_endpoint_expired: bool,
    pub is_current_device: bool,
    pub last_access_time: Option<i64>,
}

/// Enumeration for the different types of device.
///
/// Firefox Accounts seprates devices into broad categories for display purposes,
/// such as distinguishing a desktop PC from a mobile phone. Upon signin, the
/// application should inspect the device it is running on and select an appropriate
/// [`DeviceType`] to include in its device registration record.
///
#[derive(Debug)]
pub enum DeviceType {
    Desktop,
    Mobile,
    Tablet,
    VR,
    TV,
    Unknown,
}

/// Details of a web-push subscription endpoint.
///
/// This struct encapsulates the details of a web-push subscription endpoint,
/// including all the information necessary to send a notification to its owner.
/// Devices attached to the user's account may register one of these in order
/// to receive timely updates about account-related events.
///
/// Managing a web-push subscription is outside of the scope of this component.
///
#[derive(Debug)]
pub struct DevicePushSubscription {
    pub endpoint: String,
    pub public_key: String,
    pub auth_key: String,
}

/// A "capability" offered by a device.
///
/// In the FxA ecosystem, connected devices may advertize their ability to respond
/// to various "commands" that can be invoked by other devices. The details of
/// executing these commands are encapsulated as part of the FxA Client component,
/// so consumers simply need to select which ones they want to support, and can
/// use the variants of this enum to do so.
///
/// In practice, the only currently-supported command is the ability to receive a tab.
///
#[derive(Debug)]
pub enum DeviceCapability {
    SendTab,
}

/// An event that happened on the user's account.
///
/// If the application has registered a [`DevicePushSubscription`] as part of its
/// device record, then the Firefox Accounts server can send push notifications
/// about important events that happen on the user's account. This enum represents
/// the different kinds of event that can occur.
///
// Clippy suggests we Box<> the CommandReceiver variant here,
// but UniFFI isn't able to look through boxes yet, so we
// disable the warning.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum AccountEvent {
    /// Sent when another device has invoked a command for this device to execute.
    ///
    /// When receiving this event, the application should inspect the contained
    /// command and react appropriately.
    CommandReceived { command: IncomingDeviceCommand },
    /// Sent when the user has modified their account profile information.
    ///
    /// When receiving this event, the application should request fresh profile
    /// information by calling [`get_profile`](FirefoxAccount::get_profile) with
    /// `ignore_cache` set to true, and update any profile information displayed
    /// in its UI.
    ///
    ProfileUpdated,
    /// Sent when when there has been a change in authorization status.
    ///
    /// When receiving this event, the application should check whether it is
    /// still connected to the user's account by calling [`check_authorization_status`](
    /// FirefoxAccount::check_authorization_status), and updating its UI as appropriate.
    ///
    AccountAuthStateChanged,
    /// Sent when the user deletes their Firefox Account.
    ///
    /// When receiving this event, the application should act as though the user had
    /// signed out, discarding any persisted account state.
    AccountDestroyed,
    /// Sent when a new device connects to the user's account.
    ///
    /// When receiving this event, the application may use it to trigger an update
    /// of any UI that shows the list of connected devices. It may also show the
    /// user an informational notice about the new device, as a security measure.
    DeviceConnected { device_name: String },
    /// Sent when a device disconnects from the user's account.
    ///
    /// When receiving this event, the application may use it to trigger an update
    /// of any UI that shows the list of connected devices.
    DeviceDisconnected {
        device_id: String,
        is_local_device: bool,
    },
}

/// A command invoked by another device.
///
/// This enum represents all possible commands that can be invoked on
/// the device. It is the responsibility of the application to interpret
/// each command.
///
#[derive(Debug)]
pub enum IncomingDeviceCommand {
    /// Indicates that a tab has been sent to this device.
    TabReceived {
        sender: Option<Device>,
        payload: SendTabPayload,
    },
}

/// The payload sent when invoking a "send tab" command.
///
#[derive(Debug)]
pub struct SendTabPayload {
    /// The navigation history of the sent tab.
    ///
    /// The last item in this list represents the page to be displayed,
    /// while earlier items may be included in the navigation history
    /// as a convenience to the user.
    pub entries: Vec<TabHistoryEntry>,
    /// A unique identifier to be included in send-tab metrics.
    ///
    /// The application should treat this as opaque.
    pub flow_id: String,
    /// A unique identifier to be included in send-tab metrics.
    ///
    /// The application should treat this as opaque.
    pub stream_id: String,
}

/// An individual entry in the navigation history of a sent tab.
///
#[derive(Debug)]
pub struct TabHistoryEntry {
    pub title: String,
    pub url: String,
}

/// A client connected to the user's account.
///
/// This struct provides metadata about a client connected to the user's account.
/// Unlike the [`Device`] struct, "clients" encompasses both client-side and server-side
/// applications - basically anything where the user is able to sign in with their
/// Firefox Account.
///
///
/// This data would typically be used for targetted messaging purposes, catering the
/// contents of the message to what other applications the user has on their account.
///
pub struct AttachedClient {
    pub client_id: Option<String>,
    pub device_id: Option<String>,
    pub device_type: Option<DeviceType>,
    pub is_current_session: bool,
    pub name: Option<String>,
    pub created_time: Option<i64>,
    pub last_access_time: Option<i64>,
    pub scope: Option<Vec<String>>,
}

/// Information about the user that controls a Firefox Account.
///
/// This struct represents details about the user themselves, and would typically be
/// used to customize account-related UI in the browser so that it is personalize
/// for the current user.
///
pub struct Profile {
    /// The user's account uid
    ///
    /// This is an opaque immutable unique identifier for their account.
    pub uid: String,
    /// The user's current primary email address.
    ///
    /// Note that unlike the `uid` field, the email address may change over time.
    pub email: String,
    /// The user's preferred textual display name.
    pub display_name: Option<String>,
    /// The URL of a profile picture representing the user.
    ///
    /// All accounts have a corresponding profile picture. If the user has not
    /// provided one then a default image is used.
    pub avatar: String,
    /// Whether the `avatar` URL represents the default avatar image.
    pub is_default_avatar: bool,
}

/// The current state migration from legacy sign-in data.
///
/// This enum distinguishes the different states of a potential in-flight
/// migration from legacy sign-in data. A value other than [`None`](MigrationState::None)
/// indicates that there was a previously-failed migration that should be
/// retried.
///
pub enum MigrationState {
    /// No in-flight migration.
    None,
    /// An in-flight migration that will copy the sessionToken.
    CopySessionToken,
    /// An in-flight migration that will re-use the sessionToken.
    ReuseSessionToken,
}

/// Statistics about the completion of a migration from legacy sign-in data.
///
/// Applications migrating from legacy sign-in data would typically want to
/// report telemetry about whether and how that succeeded, and can use the
/// results reported in this struct to help do so.
///
#[derive(Debug)]
pub struct FxAMigrationResult {
    /// The time taken to migrate, in milliseconds.
    ///
    /// Note that this is a signed integer, for compatibility with languages
    /// that do not have unsigned integers.
    pub total_duration: i64,
}
