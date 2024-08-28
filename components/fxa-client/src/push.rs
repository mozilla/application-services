/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::handle_error;
use serde::{Deserialize, Serialize};

use crate::{internal, ApiResult, CloseTabsResult, Device, Error, FirefoxAccount, LocalDevice};

impl FirefoxAccount {
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
    ) -> ApiResult<LocalDevice> {
        self.internal
            .lock()
            .set_push_subscription(subscription.into())
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
        self.internal.lock().handle_push_message(payload)
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
        self.internal
            .lock()
            .poll_device_commands(internal::device::CommandFetchReason::Poll)?
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<_, _>>()
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
    pub fn send_single_tab(&self, target_device_id: &str, title: &str, url: &str) -> ApiResult<()> {
        self.internal
            .lock()
            .send_single_tab(target_device_id, title, url)
    }

    /// Use device commands to close one or more tabs on another device.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// If a device on the account has registered the [`CloseTabs`](DeviceCapability::CloseTabs)
    /// capability, this method can be used to close its tabs.
    #[handle_error(Error)]
    pub fn close_tabs(
        &self,
        target_device_id: &str,
        urls: Vec<String>,
    ) -> ApiResult<CloseTabsResult> {
        self.internal.lock().close_tabs(target_device_id, urls)
    }
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePushSubscription {
    pub endpoint: String,
    pub public_key: String,
    pub auth_key: String,
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
    TabsClosed {
        sender: Option<Device>,
        payload: CloseTabsPayload,
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

/// The payload sent when invoking a "close tabs" command.
#[derive(Debug)]
pub struct CloseTabsPayload {
    pub urls: Vec<String>,
}

/// An individual entry in the navigation history of a sent tab.
#[derive(Debug)]
pub struct TabHistoryEntry {
    pub title: String,
    pub url: String,
}
