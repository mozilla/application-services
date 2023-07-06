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

use crate::{ApiResult, DevicePushSubscription, Error, FirefoxAccount};
use error_support::handle_error;
use sync15::DeviceType;

impl FirefoxAccount {
    /// Create a new device record for this application.
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
    ///
    /// # Notes
    ///
    ///    - Device registration is only available to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    #[handle_error(Error)]
    pub fn initialize_device(
        &self,
        name: &str,
        device_type: DeviceType,
        supported_capabilities: Vec<DeviceCapability>,
    ) -> ApiResult<()> {
        // UniFFI doesn't have good handling of lists of references, work around it.
        let supported_capabilities: Vec<_> =
            supported_capabilities.into_iter().map(Into::into).collect();
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

    /// Get the list of known devices registered on the user's account.
    ///
    /// The application might use this information to e.g. display a list of appropriate
    /// send-tab targets.
    ///
    /// # Notes
    ///
    ///    - Device metadata is only visible to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    #[handle_error(Error)]
    pub fn get_device_list(&self) -> ApiResult<DeviceList> {
        self.internal.lock().get_device_list(false)
    }

    /// Like `get_device_list()`, but always fetch a new device list rather than using cached data.
    #[handle_error(Error)]
    pub fn refresh_device_list(&self) -> ApiResult<DeviceList> {
        self.internal.lock().get_device_list(true)
    }

    /// Get the list of devices registered on the user's account.
    ///
    /// **Deprecated**: use [FirefoxAccount::get_device_list] or
    /// [FirefoxAccount::refresh_device_list] instead.
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
    /// This information is really only useful for targeted messaging or marketing purposes,
    /// e.g. if the application wants to advertise a related product, but first wants to check
    /// whether the user is already using that product.
    ///
    /// # Notes
    ///
    ///    - Attached client metadata is only visible to applications that have been
    ///      granted the `https://identity.mozilla.com/apps/oldsync` scope.
    #[handle_error(Error)]
    pub fn get_attached_clients(&self) -> ApiResult<Vec<AttachedClient>> {
        self.internal
            .lock()
            .get_attached_clients()?
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()
    }

    /// Update the display name used for this application instance.
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
    pub fn set_device_name(&self, display_name: &str) -> ApiResult<()> {
        self.internal.lock().set_device_name(display_name)
    }

    /// Clear any custom display name used for this application instance.
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
    ) -> ApiResult<()> {
        let supported_capabilities: Vec<_> =
            supported_capabilities.into_iter().map(Into::into).collect();
        self.internal
            .lock()
            .ensure_capabilities(&supported_capabilities)
    }
}

/// List of known devices registered to the user's account
#[derive(Debug, PartialEq, Eq)]
pub struct DeviceList {
    /// The device using the FxA client
    pub local_device: Device,
    /// All other known devices
    pub remote_devices: Vec<Device>,
}

/// A device connected to the user's account.
///
/// This struct provides metadata about a device connected to the user's account.
/// This data would typically be used to display e.g. the list of candidate devices
/// in a "send tab" menu.
#[derive(Debug, PartialEq, Eq)]
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
///
/// In practice, the only currently-supported command is the ability to receive a tab.
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceCapability {
    SendTab,
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
