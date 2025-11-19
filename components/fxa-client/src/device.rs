/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Device Management
//!
//! Applications that connect to a user's account may register additional information
//! about themselves via a "device record", which allows them to:
//!
//!    - customize how they appear in the user's account management page
//!    - receive push notifications about events that happen on the account
//!    - participate in the FxA "device commands" ecosystem
//!
//! For more details on FxA device registration and management, consult the
//! [Firefox Accounts Device Registration docs](
//! https://github.com/mozilla/fxa/blob/main/packages/fxa-auth-server/docs/device_registration.md).

use error_support::handle_error;
use serde::{Deserialize, Serialize};
use sync15::DeviceType;

use crate::{ApiResult, DevicePushSubscription, Error, FirefoxAccount};

impl FirefoxAccount {
    /// Create a new device record for this application.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method register a device record for the application, providing basic metadata for
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
    #[handle_error(Error)]
    pub fn initialize_device(
        &self,
        name: &str,
        device_type: DeviceType,
        supported_capabilities: Vec<DeviceCapability>,
    ) -> ApiResult<LocalDevice> {
        // UniFFI doesn't have good handling of lists of references, work around it.
        let supported_capabilities: Vec<_> = supported_capabilities.into_iter().collect();
        self.internal
            .lock()
            .initialize_device(name, device_type, &supported_capabilities)
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
    #[handle_error(Error)]
    pub fn get_current_device_id(&self) -> ApiResult<String> {
        self.internal.lock().get_current_device_id()
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
    #[handle_error(Error)]
    pub fn get_devices(&self, ignore_cache: bool) -> ApiResult<Vec<Device>> {
        self.internal
            .lock()
            .get_devices(ignore_cache)?
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()
    }

    /// Get the list of all client applications attached to the user's account.
    ///
    /// This method returns a list of [`AttachedClient`] structs representing all the applications
    /// connected to the user's account. This includes applications that are registered as a device
    /// as well as server-side services that the user has connected.
    ///
    /// It will only return active sessions.
    /// For example, if a user has disconnected the service from their account,
    /// it wouldn't appear in this list.
    #[handle_error(Error)]
    pub fn get_attached_clients(&self) -> ApiResult<Vec<AttachedClient>> {
        self.internal
            .lock()
            .get_attached_clients()?
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()
    }

    /// Fetch and cache attached clients in the background, ignoring TTL.
    ///
    /// This method allows mobile clients to pre-warm the cache without blocking UI.
    /// Unlike [`get_attached_clients`](FirefoxAccount::get_attached_clients), this always
    /// performs a network request regardless of cache freshness.
    ///
    /// Mobile applications should call this method on a background thread when:
    /// - The user logs in
    /// - The app comes to the foreground
    /// - A push notification is received indicating account changes
    ///
    /// After calling this method, [`get_attached_clients_from_cache`](FirefoxAccount::get_attached_clients_from_cache)
    /// can be used on the main thread to get cached results without blocking.
    ///
    #[handle_error(Error)]
    pub fn refresh_attached_clients_cache(&self) -> ApiResult<()> {
        self.internal.lock().refresh_attached_clients_cache()
    }

    /// Get cached attached clients without blocking on network.
    ///
    /// This method returns the cached list of attached clients if available, even if stale.
    /// It returns `None` if no cache is available. This method never blocks on network I/O,
    /// making it safe to call from the main thread for immediate UI decisions.
    ///
    /// Mobile applications should use this method when they need to make immediate UI decisions,
    /// such as whether to show Relay options in autofill or keyboard accessories.
    ///
    /// To ensure the cache is warm, call [`refresh_attached_clients_cache`](FirefoxAccount::refresh_attached_clients_cache)
    /// proactively on a background thread.
    ///
    /// # Notes
    ///
    ///    - Returns `None` if no cached data is available
    ///    - May return stale data (cache TTL is currently 6 hours)
    ///    - Cached data is cleared on app restart and when push notifications indicate account changes
    pub fn get_attached_clients_from_cache(&self) -> Option<Vec<AttachedClient>> {
        self.internal
            .lock()
            .get_attached_clients_from_cache()
            .map(|clients| {
                clients
                    .into_iter()
                    .filter_map(|c| c.try_into().ok())
                    .collect()
            })
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
    #[handle_error(Error)]
    pub fn set_device_name(&self, display_name: &str) -> ApiResult<LocalDevice> {
        self.internal.lock().set_device_name(display_name)
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
    #[handle_error(Error)]
    pub fn clear_device_name(&self) -> ApiResult<()> {
        self.internal.lock().clear_device_name()
    }

    /// Ensure that the device record has a specific set of capabilities.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method checks that the currently-registered device record is advertising the
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
    #[handle_error(Error)]
    pub fn ensure_capabilities(
        &self,
        supported_capabilities: Vec<DeviceCapability>,
    ) -> ApiResult<LocalDevice> {
        let supported_capabilities: Vec<_> = supported_capabilities.into_iter().collect();
        self.internal
            .lock()
            .ensure_capabilities(&supported_capabilities)
    }
}

/// Device configuration
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceConfig {
    pub name: String,
    pub device_type: sync15::DeviceType,
    pub capabilities: Vec<DeviceCapability>,
}

/// Local device that's connecting to FxA
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalDevice {
    pub id: String,
    pub display_name: String,
    pub device_type: sync15::DeviceType,
    pub capabilities: Vec<DeviceCapability>,
    pub push_subscription: Option<DevicePushSubscription>,
    pub push_endpoint_expired: bool,
}

/// A device connected to the user's account.
///
/// This struct provides metadata about a device connected to the user's account.
/// This data would typically be used to display e.g. the list of candidate devices
/// in a "send tab" menu.
#[derive(Debug)]
pub struct Device {
    pub id: String,
    pub display_name: String,
    pub device_type: sync15::DeviceType,
    pub capabilities: Vec<DeviceCapability>,
    pub push_subscription: Option<DevicePushSubscription>,
    pub push_endpoint_expired: bool,
    pub is_current_device: bool,
    pub last_access_time: Option<i64>,
}

/// A "capability" offered by a device.
///
/// In the FxA ecosystem, connected devices may advertise their ability to respond
/// to various "commands" that can be invoked by other devices. The details of
/// executing these commands are encapsulated as part of the FxA Client component,
/// so consumers simply need to select which ones they want to support, and can
/// use the variants of this enum to do so.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DeviceCapability {
    SendTab,
    CloseTabs,
}

/// A client connected to the user's account.
///
/// This struct provides metadata about a client connected to the user's account.
/// Unlike the [`Device`] struct, "clients" encompasses both client-side and server-side
/// applications - basically anything where the user is able to sign in with their
/// Firefox Account.
///
///
/// This data would typically be used for targeted messaging purposes, catering the
/// contents of the message to what other applications the user has on their account.
///
pub struct AttachedClient {
    pub client_id: Option<String>,
    pub device_id: Option<String>,
    pub device_type: DeviceType,
    pub is_current_session: bool,
    pub name: Option<String>,
    pub created_time: Option<i64>,
    pub last_access_time: Option<i64>,
    pub scope: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseTabsResult {
    Ok,
    TabsNotClosed { urls: Vec<String> },
}
