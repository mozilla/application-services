/* Server Communications.
 * Handles however communication to and from the remote Push Server should be done. For Desktop
 * this will be over Websocket. For mobile, it will probably be calls into the local operating
 * system and HTTPS to the web push server.
 *
 * In the future, it could be using gRPC and QUIC, or quantum relay.
 */

#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate storage;

use std::collections::HashMap;

use storage::{Storage, Store};

pub struct RegisterResponse {
    // the UAID & Channel ID associated with the request
    pub uaid: String,
    pub channel_id: String,

    // Auth token for subsequent calls (note, only generated on new UAIDs)
    pub auth: Option<String>,

    // Push endpoint for 3rd parties
    pub endpoint: String,

    // The Sender/Group ID echoed back (if applicable.)
    pub senderid: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum BroadcastValue {
    Value(String),
    Nested(HashMap<String, BroadcastValue>),
}

/* TODO: Fill these out with Failures
 */
pub struct ConnectionError;

pub trait Connection {
    // Generate a new connection & send a "hello"
    fn connect<C: Connection>(
        options: Option<HashMap<String, String>>,
    ) -> Result<C, ConnectionError>;

    // get the connection UAID
    fn uaid(&self) -> String;

    // reset UAID. This causes all known subscriptions to be reset.
    fn reset_uaid<S: Storage>(&self, storage: &S) -> Result<bool, ConnectionError>;

    // send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &self,
        channel_id: &str,
        vapid_public_key: Option<&str>,
        registration_token: Option<&str>,
    ) -> Result<RegisterResponse, ConnectionError>;

    // Drop an endpoint
    fn unsubscribe(&self, channel_id: &str, auth: &str) -> Result<bool, ConnectionError>;

    // Update an endpoint with new info
    fn update(
        &self,
        channel_id: &str,
        auth: &str,
        new_token: &str,
    ) -> Result<bool, ConnectionError>;

    // Get a list of server known channels. If it differs from what we have, reset the UAID, and refresh channels.
    // Should be done once a day.
    fn channel_list(&self) -> Vec<String>;

    // Add one or more new broadcast subscriptions.
    fn broadcast_subscribe(
        &self,
        broadcast: BroadcastValue,
    ) -> Result<BroadcastValue, ConnectionError>;

    // get the list of broadcasts
    fn broadcasts(&self) -> Result<BroadcastValue, ConnectionError>;

    //impl TODO: Handle a Ping response with updated Broadcasts.
    //impl TODO: Handle an incoming Notification
}

pub struct Connect {
    options: Option<HashMap<String, String>>,
    store: Store,
    pub auth: String, // Server auth token
}

impl Connection for Connect {
    fn connect<C: Connection>(
        options: Option<HashMap<String, String>>,
    ) -> Result<C, ConnectionError> {
        Err(ConnectionError)
    }

    fn uaid(&self) -> String {
        String::from("deadbeef00000000decafbad00000000")
    }

    // reset UAID. This causes all known subscriptions to be reset.
    fn reset_uaid<S: Storage>(&self, storage: &S) -> Result<bool, ConnectionError> {
        Err(ConnectionError)
    }

    // send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &self,
        channel_id: &str,
        vapid_public_key: Option<&str>,
        registration_token: Option<&str>,
    ) -> Result<RegisterResponse, ConnectionError> {
        Err(ConnectionError)
    }

    // Drop an endpoint
    fn unsubscribe(&self, channel_id: &str, auth: &str) -> Result<bool, ConnectionError> {
        Err(ConnectionError)
    }

    // Update an endpoint with new info
    fn update(
        &self,
        channel_id: &str,
        auth: &str,
        new_token: &str,
    ) -> Result<bool, ConnectionError> {
        Err(ConnectionError)
    }

    // Get a list of server known channels. If it differs from what we have, reset the UAID, and refresh channels.
    // Should be done once a day.
    fn channel_list(&self) -> Vec<String> {
        Vec::new()
    }

    // Add one or more new broadcast subscriptions.
    fn broadcast_subscribe(
        &self,
        broadcast: BroadcastValue,
    ) -> Result<BroadcastValue, ConnectionError> {
        Err(ConnectionError)
    }

    // get the list of broadcasts
    fn broadcasts(&self) -> Result<BroadcastValue, ConnectionError> {
        Err(ConnectionError)
    }

    //impl TODO: Handle a Ping response with updated Broadcasts.
    //impl TODO: Handle an incoming Notification
}
