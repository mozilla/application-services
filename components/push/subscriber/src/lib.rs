/* Handle external Push Subscription Requests.
 * "priviledged" system calls may require additional handling and should be flagged as such.
 */

extern crate serde_json;

extern crate communications;
extern crate crypto;
extern crate storage;

use std::collections::HashMap;

use communications::{Connect, Connection, ConnectionError};
use crypto::{Crypto, Cryptography, Key};
use storage::{ChannelID, Storage, Store};

pub struct SubscriptionError;

pub struct SubscriptionKeys {
    pub auth: Vec<u8>,
    pub p256dh: Vec<u8>,
}

// Subscription structure
pub struct Subscription {
    pub channelid: ChannelID,
    pub endpoint: String,
    pub keys: SubscriptionKeys,
}

pub trait Subscriber {
    // get a new subscription (including keys, endpoint, etc.)
    // note if this is a "priviledged" system call that does not require additional decryption
    fn get_subscription<S: Storage>(
        storage: S,
        origin_attributes: HashMap<String, String>, // Does this include the origin proper?
        app_server_key: Option<&str>,               // Passed to server.
        registration_key: Option<&str>,             // Local OS push registration ID
        priviledged: bool,                          // Is this a system call / skip encryption?
    ) -> Result<Subscription, SubscriptionError>;

    // Update an existing subscription (change bridge endpoint)
    fn update_subscription<S: Storage>(
        storage: S,
        chid: ChannelID,
        bridge_id: Option<String>,
    ) -> Result<Subscription, SubscriptionError>;

    // remove a subscription
    fn del_subscription<S: Storage>(store: S, chid: ChannelID) -> Result<bool, SubscriptionError>;

    // to_json -> impl Into::<String> for Subscriber...
}

impl Subscriber for Subscription {
    fn get_subscription<S: Storage>(
        storage: S,
        origin_attributes: HashMap<String, String>,
        app_server_key: Option<&str>,
        registration_key: Option<&str>,
        priviledged: bool,
    ) -> Result<Subscription, SubscriptionError> {
        if let Ok(con) = Connect::connect::<Connect>(None) {
            let uaid = con.uaid();
            let chid = storage.generate_channel_id();
            if let Ok(endpoint_data) = con.subscribe(&chid, app_server_key, registration_key) {
                let private_key = Crypto::generate_key().unwrap();
                storage.create_record(
                    &uaid,
                    &chid,
                    origin_attributes,
                    &endpoint_data.endpoint,
                    &con.auth,
                    &private_key,
                    priviledged,
                );
                return Ok(Subscription {
                    channelid: chid,
                    endpoint: endpoint_data.endpoint.clone(),
                    keys: SubscriptionKeys {
                        p256dh: private_key.public.clone(),
                        auth: private_key.auth.clone(),
                    },
                });
            }
        }
        Err(SubscriptionError)
    }

    fn update_subscription<S: Storage>(
        storage: S,
        chid: ChannelID,
        bridge_id: Option<String>,
    ) -> Result<Subscription, SubscriptionError> {
        Err(SubscriptionError)
    }

    // remove a subscription
    fn del_subscription<S: Storage>(store: S, chid: ChannelID) -> Result<bool, SubscriptionError> {
        Ok(false)
    }
}
