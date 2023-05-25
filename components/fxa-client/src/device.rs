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
use sync15::DeviceType;
use crate::{internal, ApiResult, Error, FirefoxAccount};

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
        Ok(self.internal.lock().unwrap().initialize_device(
            name,
            device_type,
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
    #[handle_error(Error)]
    pub fn get_current_device_id(&self) -> ApiResult<String> {
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
    #[handle_error(Error)]
    pub fn get_devices(&self, ignore_cache: bool) -> ApiResult<Vec<Device>> {
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
    #[handle_error(Error)]
    pub fn set_device_name(&self, display_name: &str) -> ApiResult<()> {
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
    #[handle_error(Error)]
    pub fn clear_device_name(&self) -> ApiResult<()> {
        Ok(self.internal.lock().unwrap().clear_device_name()?)
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
    ) -> ApiResult<()> {
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
    #[handle_error(Error)]
    pub fn set_push_subscription(
        &self,
        subscription: DevicePushSubscription,
    ) -> ApiResult<()> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .set_push_subscription(subscription.into())?)
    }

    /// Process and respond to a server-delivered account update message
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications should call this method whenever they receive a push notification from the Firefox Accounts server.
    /// Such messages typically indicate a noteworthy change of state on the user's account, such as an update to their profile information
    /// or the disconnection of a client. The [`FirefoxAccount`] struct will update its internal state
    /// accordingly and return an individual [`AccountEvent`] struct describing the event, which the application
    /// may use for further processing.
    ///
    /// It's important to note if the event is [`AccountEvent::CommandReceived`], the caller should call
    /// [`FirefoxAccount::poll_device_commands`]
    #[handle_error(Error)]
    pub fn handle_push_message(&self, payload: &str) -> ApiResult<AccountEvent> {
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
    #[handle_error(Error)]
    pub fn poll_device_commands(&self) -> ApiResult<Vec<IncomingDeviceCommand>> {
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
    #[handle_error(Error)]
    pub fn send_single_tab(
        &self,
        target_device_id: &str,
        title: &str,
        url: &str,
    ) -> ApiResult<()> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .send_single_tab(target_device_id, title, url)?)
    }
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
/// In the FxA ecosystem, connected devices may advertise their ability to respond
/// to various "commands" that can be invoked by other devices. The details of
/// executing these commands are encapsulated as part of the FxA Client component,
/// so consumers simply need to select which ones they want to support, and can
/// use the variants of this enum to do so.
///
/// In practice, the only currently-supported command is the ability to receive a tab.
#[derive(Debug)]
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

    /// An unknown event, most likely an event the client doesn't support yet.
    ///
    /// When receiving this event, the application should gracefully ignore it.
    Unknown,
}

/// A command invoked by another device.
///
/// This enum represents all possible commands that can be invoked on
/// the device. It is the responsibility of the application to interpret
/// each command.
#[derive(Debug)]
pub enum IncomingDeviceCommand {
    /// Indicates that a tab has been sent to this device.
    TabReceived {
        sender: Option<Device>,
        payload: SendTabPayload,
    },
}

/// The payload sent when invoking a "send tab" command.
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
#[derive(Debug)]
pub struct TabHistoryEntry {
    pub title: String,
    pub url: String,
}
