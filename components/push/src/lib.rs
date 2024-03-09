/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
//! # Rust Push Component
//!
//! This component helps an application to manage [WebPush](https://developer.mozilla.org/en-US/docs/Web/API/Push_API) subscriptions,
//! acting as an intermediary between Mozilla's [autopush service](https://autopush.readthedocs.io/en/latest/)
//! and platform native push infrastructure such as [Firebase Cloud Messaging](https://firebase.google.com/docs/cloud-messaging) or [Amazon Device Messaging](https://developer.amazon.com/docs/adm/overview.html).
//!
//! ## Background Concepts
//!
//! ### WebPush Subscriptions
//!
//! A WebPush client manages a number of *subscriptions*, each of which is used to deliver push
//! notifications to a different part of the app. For example, a web browser might manage a separate
//! subscription for each website that has registered a [service worker](https://developer.mozilla.org/en-US/docs/Web/API/Service_Worker_API), and an application that includes Firefox Accounts would manage
//! a dedicated subscription on which to receive account state updates.
//!
//! Each subscription is identified by a unique *channel id*, which is a randomly-generated identifier.
//! It's the responsibility of the application to know how to map a channel id to an appropriate function
//! in the app to receive push notifications. Subscriptions also have an associated *scope* which is something
//! to do which service workers that your humble author doesn't really understand :-/.
//!
//! When a subscription is created for a channel id, we allocate *subscription info* consisting of:
//!
//! * An HTTP endpoint URL at which push messages can be submitted.
//! * A cryptographic key and authentication secret with which push messages can be encrypted.
//!
//! This subscription info is distributed to other services that want to send push messages to
//! the application.
//!
//! The HTTP endpoint is provided by Mozilla's [autopush service](https://autopush.readthedocs.io/en/latest/),
//! and we use the [rust-ece](https://github.com/mozilla/rust-ece) to manage encryption with the cryptographic keys.
//!
//! Here's a helpful diagram of how the *subscription* flow works at a high level across the moving parts:
//! ![A Sequence diagram showing how the different parts of push interact](https://mozilla.github.io/application-services/book/diagrams/Push-Component-Subscription-flow.png "Sequence diagram")
//!
//! ### AutoPush Bridging
//!
//! Our target consumer platforms each have their own proprietary push-notification infrastructure,
//! such as [Firebase Cloud Messaging](https://firebase.google.com/docs/cloud-messaging) for Android
//! and the [Apple Push Notification Service](https://developer.apple.com/notifications/) for iOS.
//! Mozilla's [autopush service](https://autopush.readthedocs.io/en/latest/) provides a bridge between
//! these different mechanisms and the WebPush standard so that they can be used with a consistent
//! interface.
//!
//! This component acts a client of the [Push Service Bridge HTTP Interface](https://autopush.readthedocs.io/en/latest/http.html#push-service-bridge-http-interface).
//!
//! We assume two things about the consuming application:
//! * It has registered with the autopush service and received a unique `app_id` identifying this registration.
//! * It has registered with whatever platform-specific notification infrastructure is appropriate, and is
//!   able to obtain a `token` corresponding to its native push notification state.
//!
//! On first use, this component will register itself as an *application instance* with the autopush service, providing the `app_id` and `token` and receiving a unique `uaid` ("user-agent id") to identify its
//! connection to the server.
//!
//! As the application adds or removes subscriptions using the API of this component, it will:
//! * Manage a local database of subscriptions and the corresponding cryptographic material.
//! * Make corresponding HTTP API calls to update the state associated with its `uaid` on the autopush server.
//!
//! Periodically, the application should call a special `verify_connection` method to check whether
//! the state on the autopush server matches the local state and take any corrective action if it
//! differs.
//!
//! For local development and debugging, it is possible to run a local instance of the autopush
//! bridge service; see [this google doc](https://docs.google.com/document/d/18L_g2hIj_1mncF978A_SHXN4udDQLut5P_ZHYZEwGP8) for details.
//!
//! ## API
//!
//! ## Initialization
//!
//! Calls are handled by the `PushManager`, which provides a handle for future calls.
//!
//! example:
//! ```kotlin
//!
//! import mozilla.appservices.push.(PushManager, BridgeTypes)
//!
//! // The following are mock calls for fetching application level configuration options.
//! // "SenderID" is the native OS push message application identifier. See Native
//! // messaging documentation for details.
//! val sender_id = SystemConfigurationOptions.get("SenderID")
//!
//! // The "bridge type" is the identifier for the native OS push message system.
//! // (e.g. FCM for Google Firebase Cloud Messaging, ADM for Amazon Direct Messaging,
//! // etc.)
//! val bridge_type = BridgeTypes.FCM
//!
//! // The "registration_id" is the native OS push message user registration number.
//! // Native push message registration usually happens at application start, and returns
//! // an opaque user identifier string. See Native messaging documentation for details.
//! val registration_id = NativeMessagingSystem.register(sender_id)
//!
//! val push_manager = PushManager(
//!     sender_id,
//!     bridge_type,
//!     registration_id
//! )
//!
//! // It is strongly encouraged that the connection is verified at least once a day.
//! // This will ensure that the server and UA have matching information regarding
//! // subscriptions. This call usually returns quickly, but may take longer if the
//! // UA has a large number of subscriptions and things have fallen out of sync.
//!
//! for change in push_manager.verify_connection() {
//!     // fetch the subscriber from storage using the change[0] and
//!     // notify them with a `pushsubscriptionchange` message containing the new
//!     // endpoint change[1]
//! }
//!
//! ```
//!
//! ## New subscription
//!
//! Before messages can be delivered, a new subscription must be requested. The subscription info block contains all the information a remote subscription provider service will need to encrypt and transmit a message to this user agent.
//!
//! example:
//! ```kotlin
//!
//! // Each new request must have a unique "channel" identifier. This channel helps
//! // later identify recipients and aid in routing. A ChannelID is a UUID4 value.
//! // the "scope" is the ServiceWorkerRegistration scope. This will be used
//! // later for push notification management.
//! val channelID = GUID.randomUUID()
//!
//! val subscription_info = push_manager.subscribe(channelID, endpoint_scope)
//!
//! // the published subscription info has the following JSON format:
//! // {"endpoint": subscription_info.endpoint,
//! //  "keys": {
//! //      "auth": subscription_info.keys.auth,
//! //      "p256dh": subscription_info.keys.p256dh
//! //  }}
//! ```
//!
//! ## End a subscription
//!
//! A user may decide to no longer receive a given subscription. To remove a given subscription, pass the associated channelID
//!
//! ```kotlin
//! push_manager.unsubscribe(channelID)  // Terminate a single subscription
//! ```
//!
//! If the user wishes to terminate all subscriptions, send and empty string for channelID
//!
//! ```kotlin
//! push_manager.unsubscribe("")        // Terminate all subscriptions for a user
//! ```
//!
//! If this function returns `false` the subsequent `verify_connection` may result in new channel endpoints.
//!
//! ## Decrypt an incoming subscription message
//!
//! An incoming subscription body will contain a number of metadata elements along with the body of the message. Due to platform differences, how that metadata is provided may //! vary, however the most common form is that the messages "payload" looks like.
//!
//! ```javascript
//! {"chid": "...",         // ChannelID
//!  "con": "...",          // Encoding form
//!  "enc": "...",          // Optional encryption header
//!  "crypto-key": "...",   // Optional crypto key header
//!  "body": "...",         // Encrypted message body
//! }
//! ```
//! These fields may be included as a sub-hash, or may be intermingled with other data fields. If you have doubts or concerns, please contact the Application Services team guidance
//!
//! Based on the above payload, an example call might look like:
//!
//! ```kotlin
//!     val result = manager.decrypt(
//!         channelID = payload["chid"].toString(),
//!         body = payload["body"].toString(),
//!         encoding = payload["con"].toString(),
//!         salt = payload.getOrElse("enc", "").toString(),
//!         dh = payload.getOrElse("dh", "").toString()
//!     )
//!     // result returns a byte array. You may need to convert to a string
//!     return result.toString(Charset.forName("UTF-8"))
//!```

uniffi::include_scaffolding!("push");
// All implementation detail lives in the `internal` module
mod internal;
use std::{collections::HashMap, sync::Mutex};
mod error;

use error_support::handle_error;
pub use internal::config::{BridgeType, Protocol as PushHttpProtocol, PushConfiguration};
use internal::crypto::Crypto;
use internal::{communications::ConnectHttp, push_manager::DecryptResponse};

pub use error::{debug, ApiResult, PushApiError, PushError};
use internal::storage::Store;

/// Object representing the PushManager used to manage subscriptions
///
/// The `PushManager` object is the main interface provided by this crate
/// it allow consumers to manage push subscriptions. It exposes methods that
/// interact with the [`autopush server`](https://autopush.readthedocs.io/en/latest/)
/// and persists state representing subscriptions.
pub struct PushManager {
    // We serialize all access on a mutex for thread safety
    // TODO: this can improved by making the locking more granular
    // and moving the mutex down to ensure `internal::PushManager`
    // is Sync + Send
    internal: Mutex<internal::PushManager<ConnectHttp, Crypto, Store>>,
}

impl PushManager {
    /// Creates a new [`PushManager`] object, not subscribed to any
    /// channels
    ///
    /// # Arguments
    ///   - `config`: [`PushConfiguration`] the configuration for this instance of PushManager
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - PushManager is unable to open the `database_path` given
    ///   - PushManager is unable to establish a connection to the autopush server
    #[handle_error(PushError)]
    pub fn new(config: PushConfiguration) -> ApiResult<Self> {
        debug!(
            "PushManager server_host: {}, http_protocol: {}",
            config.server_host, config.http_protocol
        );
        Ok(Self {
            internal: Mutex::new(internal::PushManager::new(config)?),
        })
    }

    /// Subscribes to a new channel and gets the Subscription Info block
    ///
    /// # Arguments
    ///   - `channel_id` - Channel ID (UUID4) for new subscription, either pre-generated or "" and one will be created.
    ///   - `scope` - Site scope string (defaults to "" for no site scope string).
    ///   - `server_key` - optional VAPID public key to "lock" subscriptions (defaults to "" for no key)
    ///
    /// # Returns
    /// A Subscription response that includes the following:
    ///   - A URL that can be used to deliver push messages
    ///   - A cryptographic key that can be used to encrypt messages
    ///     that would then be decrypted using the [`PushManager::decrypt`] function
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - PushManager was unable to access its persisted storage
    ///   - An error occurred sending a subscription request to the autopush server
    ///   - An error occurred generating or deserializing the cryptographic keys
    #[handle_error(PushError)]
    pub fn subscribe(
        &self,
        scope: &str,
        server_key: &Option<String>,
    ) -> ApiResult<SubscriptionResponse> {
        self.internal
            .lock()
            .unwrap()
            .subscribe(scope, server_key.as_deref())
    }

    /// Retrieves an existing push subscription
    ///
    /// # Arguments
    ///   - `scope` - Site scope string
    ///
    /// # Returns
    /// A Subscription response that includes the following:
    ///   - A URL that can be used to deliver push messages
    ///   - A cryptographic key that can be used to encrypt messages
    ///     that would then be decrypted using the [`PushManager::decrypt`] function
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - PushManager was unable to access its persisted storage
    ///   - An error occurred generating or deserializing the cryptographic keys
    #[handle_error(PushError)]
    pub fn get_subscription(&self, scope: &str) -> ApiResult<Option<SubscriptionResponse>> {
        self.internal.lock().unwrap().get_subscription(scope)
    }

    /// Unsubscribe from given channelID, ending that subscription for the user.
    ///
    /// # Arguments
    ///   - `channel_id` - Channel ID (UUID) for subscription to remove
    ///
    /// # Returns
    /// Returns a boolean. Boolean is False if the subscription was already
    /// terminated in the past.
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an unsubscribe request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    #[handle_error(PushError)]
    pub fn unsubscribe(&self, channel_id: &str) -> ApiResult<bool> {
        self.internal.lock().unwrap().unsubscribe(channel_id)
    }

    /// Unsubscribe all channels for the user
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an unsubscribe request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    #[handle_error(PushError)]
    pub fn unsubscribe_all(&self) -> ApiResult<()> {
        self.internal.lock().unwrap().unsubscribe_all()
    }

    /// Updates the Native OS push registration ID.
    ///
    /// # Arguments:
    ///   - `new_token` - the new Native OS push registration ID
    /// # Errors
    /// Return an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an update request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    #[handle_error(PushError)]
    pub fn update(&self, new_token: &str) -> ApiResult<()> {
        self.internal.lock().unwrap().update(new_token)
    }

    /// Verifies the connection state
    ///
    /// **NOTE**: This does not resubscribe to any channels
    /// it only returns the list of channels that the client should
    /// re-subscribe to.
    ///
    /// # Arguments
    ///   - `force_verify`: Force verification and ignore the rate limiter
    ///
    /// # Returns
    /// Returns a list of [`PushSubscriptionChanged`]
    /// indicating the channels the consumer the client should re-subscribe
    /// to. If the list is empty, the client's connection was verified
    /// successfully, and the client does not need to resubscribe
    ///
    /// # Errors
    /// Return an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an channel list retrieval request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    #[handle_error(PushError)]
    pub fn verify_connection(&self, force_verify: bool) -> ApiResult<Vec<PushSubscriptionChanged>> {
        self.internal
            .lock()
            .unwrap()
            .verify_connection(force_verify)
    }

    /// Decrypts a raw push message.
    ///
    /// This accepts the content of a Push Message (from websocket or via Native Push systems).
    /// # Arguments:
    ///   - `channel_id` - the ChannelID (included in the envelope of the message)
    ///   - `body` - The encrypted body of the message
    ///   - `encoding` - The Content Encoding "enc" field of the message (defaults to "aes128gcm")
    ///   - `salt` - The "salt" field (if present in the raw message, defaults to "")
    ///   - `dh` - The "dh" field (if present in the raw message, defaults to "")
    ///
    /// # Returns
    /// Decrypted message body as a signed byte array
    /// they byte array is signed to allow consumers (Kotlin only at the time of this documentation)
    /// to work easily with the message. (They can directly call `.toByteArray` on it)
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - There are no records associated with the UAID the [`PushManager`] contains
    ///   - An error occurred while decrypting the message
    ///   - An error occurred accessing the PushManager's persisted storage
    #[handle_error(PushError)]
    pub fn decrypt(&self, payload: HashMap<String, String>) -> ApiResult<DecryptResponse> {
        self.internal.lock().unwrap().decrypt(payload)
    }
}

/// Key Information that can be used to encrypt payloads. These are encoded as base64
/// so will need to be decoded before they can actually be used as keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyInfo {
    pub auth: String,
    pub p256dh: String,
}
/// Subscription Information, the endpoint to send push messages to and
/// the key information that can be used to encrypt payloads
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubscriptionInfo {
    pub endpoint: String,
    pub keys: KeyInfo,
}

/// The subscription response object returned from [`PushManager::subscribe`]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubscriptionResponse {
    pub channel_id: String,
    pub subscription_info: SubscriptionInfo,
}

/// An dictionary describing the push subscription that changed, the caller
/// will receive a list of [`PushSubscriptionChanged`] when calling
/// [`PushManager::verify_connection`], one entry for each channel that the
/// caller should resubscribe to
#[derive(Debug, Clone)]
pub struct PushSubscriptionChanged {
    pub channel_id: String,
    pub scope: String,
}
