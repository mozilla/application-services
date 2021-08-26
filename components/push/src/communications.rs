/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Server Communications.
//!
//! Handles however communication to and from the remote Push Server should be done. For Desktop
//! this will be over Websocket. For mobile, it will probably be calls into the local operating
//! system and HTTPS to the web push server.
//!
//! In the future, it could be using gRPC and QUIC, or quantum relay.

use serde_derive::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use url::Url;
use viaduct::{header_names, status_codes, Headers, Request};

use crate::config::PushConfiguration;
use crate::error::ErrorKind;
use crate::error::{
    self,
    ErrorKind::{
        AlreadyRegisteredError, CommunicationError, CommunicationServerError,
        UAIDNotRecognizedError,
    },
};
use crate::storage::Store;

mod rate_limiter;
pub use rate_limiter::PersistedRateLimiter;

const UAID_NOT_FOUND_ERRNO: u32 = 103;
#[derive(Debug)]
pub struct RegisterResponse {
    /// The UAID associated with the request
    pub uaid: String,

    /// The Channel ID associated with the request
    pub channel_id: String,

    /// Auth token for subsequent calls (note, only generated on new UAIDs)
    pub secret: Option<String>,

    /// Push endpoint for 3rd parties
    pub endpoint: String,

    /// The Sender/Group ID echoed back (if applicable.)
    pub senderid: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum BroadcastValue {
    Value(String),
    Nested(HashMap<String, BroadcastValue>),
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
    NotConnected,
    NeedsReconnection,
    Connected {
        uaid: String,
        auth: Option<String>, // Server auth token
    },
}

impl ConnectionState {
    fn is_connected(&self) -> bool {
        if let ConnectionState::Connected { .. } = self {
            return true;
        }
        false
    }
}
/// A new communication link to the Autopush server
pub trait Connection {
    // get the connection UAID
    // TODO [conv]: reset_uaid(). This causes all known subscriptions to be reset.

    /// send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &mut self,
        channel_id: &str,
        app_server_key: Option<&str>,
    ) -> error::Result<RegisterResponse>;

    /// Drop an endpoint
    fn unsubscribe(&self, channel_id: &str) -> error::Result<bool>;

    /// drop all endpoints
    fn unsubscribe_all(&mut self) -> error::Result<bool>;

    /// Update the autopush server with the new native OS Messaging authorization token
    fn update(&mut self, new_token: &str) -> error::Result<bool>;

    /// Get a list of server known channels.
    fn channel_list(&self) -> error::Result<Vec<String>>;

    /// Verify that the known channel list matches up with the server list. If this fails, regenerate endpoints.
    /// This should be performed once a day.
    fn verify_connection(&mut self, channels: &[String]) -> error::Result<bool>;

    /// Add one or more new broadcast subscriptions.
    fn broadcast_subscribe(&self, broadcast: BroadcastValue) -> error::Result<BroadcastValue>;

    /// get the list of broadcasts
    fn broadcasts(&self) -> error::Result<BroadcastValue>;

    //impl TODO: Handle a Ping response with updated Broadcasts.
    //impl TODO: Handle an incoming Notification
}

/// Connect to the Autopush server via the HTTP interface
pub struct ConnectHttp {
    pub options: PushConfiguration,
    pub state: ConnectionState,
}

// Connect to the Autopush server
pub fn connect(options: PushConfiguration, state: ConnectionState) -> error::Result<ConnectHttp> {
    // find connection via options

    if options.socket_protocol.is_some() && options.http_protocol.is_some() {
        return Err(
            CommunicationError("Both socket and HTTP protocols cannot be set.".to_owned()).into(),
        );
    };
    if options.socket_protocol.is_some() {
        return Err(CommunicationError("Unsupported".to_owned()).into());
    };
    if options.bridge_type.is_some() && options.registration_id.is_none() {
        return Err(CommunicationError(
            "Missing Registration ID, please register with OS first".to_owned(),
        )
        .into());
    };
    let connection = ConnectHttp { options, state };

    Ok(connection)
}

impl ConnectHttp {
    fn headers(&self) -> error::Result<Headers> {
        let mut headers = Headers::new();
        if let ConnectionState::Connected {
            auth: Some(ref auth),
            ..
        } = &self.state
        {
            headers
                .insert(header_names::AUTHORIZATION, format!("webpush {}", auth))
                .map_err(|e| {
                    error::ErrorKind::CommunicationError(format!("Header error: {:?}", e))
                })?;
        };
        Ok(headers)
    }

    fn check_response_error(&self, response: &viaduct::Response) -> error::Result<()> {
        // An error response, the extended object structure is retrieved from
        // https://autopush.readthedocs.io/en/latest/http.html#response
        #[derive(Deserialize)]
        struct ResponseError {
            pub errno: u32,
            pub message: String,
        }
        if response.is_server_error() {
            let response_error = response.json::<ResponseError>()?;
            return Err(CommunicationServerError(format!(
                "General Server Error: {}",
                response_error.message
            ))
            .into());
        }
        if response.is_client_error() {
            let response_error = response.json::<ResponseError>()?;
            if response.status == status_codes::CONFLICT {
                return Err(AlreadyRegisteredError.into());
            }
            if response.status == status_codes::GONE && response_error.errno == UAID_NOT_FOUND_ERRNO
            {
                return Err(UAIDNotRecognizedError(response_error.message).into());
            }
            return Err(
                CommunicationError(format!("Unhandled client error {:?}", response)).into(),
            );
        }
        Ok(())
    }

    fn format_unsubscribe_url(&self, uaid: &str) -> String {
        format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &self.options.http_protocol.as_ref().unwrap(),
            &self.options.server_host,
            &self.options.bridge_type.as_ref().unwrap(),
            &self.options.sender_id,
            uaid,
        )
    }
}

impl Connection for ConnectHttp {
    /// send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &mut self,
        channel_id: &str,
        app_server_key: Option<&str>,
    ) -> error::Result<RegisterResponse> {
        // check that things are set
        if self.options.http_protocol.is_none() || self.options.bridge_type.is_none() {
            return Err(
                CommunicationError("Bridge type or application id not set.".to_owned()).into(),
            );
        }
        let options = self.options.clone();
        let bridge_type = &options.bridge_type.unwrap();
        let mut url = format!(
            "{}://{}/v1/{}/{}/registration",
            &options.http_protocol.unwrap(),
            &options.server_host,
            &bridge_type,
            &options.sender_id
        );
        // Add the Authorization header if we have a prior subscription.
        if let ConnectionState::Connected { uaid, .. } = &self.state {
            url.push('/');
            url.push_str(uaid);
            url.push_str("/subscription");
        }
        let mut body = HashMap::new();
        body.insert("token", options.registration_id.unwrap());
        body.insert("channelID", channel_id.to_owned());
        if let Some(key) = app_server_key {
            body.insert("key", key.to_owned());
        }
        // for unit tests, we shouldn't call the server. This is because we would need to create
        // a valid FCM test senderid (and make sure we call it), etc. There has also been a
        // history of problems where doing this tends to fail because of uncontrolled, third party
        // system reliability issues making the tree turn orange.
        if &self.options.sender_id == "test" {
            let uaid = "abad1d3a00000000aabbccdd00000000";
            let auth = "LsuUOBKVQRY6-l7_Ajo-Ag";
            self.state = ConnectionState::Connected {
                uaid: uaid.to_owned(),
                auth: Some(auth.to_owned()),
            };
            return Ok(RegisterResponse {
                uaid: uaid.to_owned(),
                channel_id: "deadbeef00000000decafbad00000000".to_owned(),
                secret: Some(auth.to_owned()),
                endpoint: "http://push.example.com/test/opaque".to_owned(),
                senderid: Some(self.options.sender_id.clone()),
            });
        }
        let url = Url::parse(&url)?;
        let response = Request::post(url)
            .headers(self.headers()?)
            .json(&body)
            .send()?;
        self.check_response_error(&response)?;
        let response: Value = response.json()?;
        if !self.state.is_connected() {
            let uaid = response["uaid"].as_str().ok_or_else(|| {
                CommunicationServerError(
                    "General Server Error: Server did not send back the uaid".to_owned(),
                )
            })?;
            let auth = response["secret"].as_str().map(ToString::to_string);
            self.state = ConnectionState::Connected {
                uaid: uaid.to_owned(),
                auth,
            };
        }

        let channel_id = response["channelID"].as_str().map(ToString::to_string);
        let endpoint = response["endpoint"].as_str().map(ToString::to_string);

        if let ConnectionState::Connected { uaid, auth } = &self.state {
            return Ok(RegisterResponse {
                uaid: uaid.to_owned(),
                channel_id: channel_id.unwrap(),
                secret: auth.to_owned(),
                endpoint: endpoint.unwrap(),
                senderid: response["senderid"].as_str().map(ToString::to_string),
            });
        }
        Err(ErrorKind::GeneralError(
            "Client did not successfully connect after the server responded".into(),
        )
        .into())
    }

    /// Drop a channel and stop receiving updates.
    fn unsubscribe(&self, channel_id: &str) -> error::Result<bool> {
        if &self.options.sender_id == "test" {
            return Ok(true);
        }
        match &self.state {
            ConnectionState::Connected { uaid, .. } => {
                let url = format!(
                    "{}/subscription/{}",
                    self.format_unsubscribe_url(uaid),
                    channel_id
                );
                let response = Request::delete(Url::parse(&url)?)
                    .headers(self.headers()?)
                    .send()?;
                self.check_response_error(&response)?;
                Ok(true)
            }
            ConnectionState::NeedsReconnection => Err(ErrorKind::ReconnectError.into()),
            ConnectionState::NotConnected => {
                Err(CommunicationError("Client not connected".to_owned()).into())
            }
        }
    }

    /// Drops all channels and stops receiving notifications.
    /// this also wipes the `uaid` and the `auth` fields.
    fn unsubscribe_all(&mut self) -> error::Result<bool> {
        if &self.options.sender_id == "test" {
            return Ok(true);
        }
        match &self.state {
            ConnectionState::Connected { uaid, .. } => {
                let url = self.format_unsubscribe_url(uaid);
                let response = Request::delete(Url::parse(&url)?)
                    .headers(self.headers()?)
                    .send()?;
                self.check_response_error(&response)?;
                self.state = ConnectionState::NeedsReconnection;
                Ok(true)
            }
            ConnectionState::NeedsReconnection => Err(ErrorKind::ReconnectError.into()),
            ConnectionState::NotConnected => {
                Err(CommunicationError("Client not connected".to_owned()).into())
            }
        }
    }

    /// Update the push server with the new OS push authorization token
    fn update(&mut self, new_token: &str) -> error::Result<bool> {
        if self.options.sender_id == "test" {
            self.state = ConnectionState::Connected {
                uaid: "abad1d3a00000000aabbccdd00000000".to_owned(),
                auth: Some("LsuUOBKVQRY6-l7_Ajo-Ag".to_owned()),
            };
            return Ok(true);
        }
        match &self.state {
            ConnectionState::Connected { uaid, .. } => {
                self.options.registration_id = Some(new_token.to_owned());
                let url = format!(
                    "{}://{}/v1/{}/{}/registration/{}",
                    self.options.http_protocol.as_ref().unwrap(),
                    &self.options.server_host,
                    self.options.bridge_type.as_ref().unwrap(),
                    &self.options.sender_id,
                    &uaid
                );
                let mut body = HashMap::new();
                body.insert("token", new_token);
                let response = Request::put(Url::parse(&url)?)
                    .json(&body)
                    .headers(self.headers()?)
                    .send()?;
                self.check_response_error(&response)?;
                Ok(true)
            }
            ConnectionState::NeedsReconnection => Err(ErrorKind::ReconnectError.into()),
            ConnectionState::NotConnected => {
                Err(CommunicationError("Client not connected".to_owned()).into())
            }
        }
    }

    /// Get a list of server known channels. If it differs from what we have, reset the UAID, and refresh channels.
    /// Should be done once a day.
    fn channel_list(&self) -> error::Result<Vec<String>> {
        #[derive(Deserialize, Debug)]
        struct Payload {
            uaid: String,
            #[serde(rename = "channelIDs")]
            channel_ids: Vec<String>,
        }

        match &self.state {
            ConnectionState::Connected { uaid, .. } => {
                if self.options.bridge_type.is_none() {
                    return Err(CommunicationError("No Bridge Type set".into()).into());
                }
                let https = "https".to_owned();
                let url = format!(
                    "{}://{}/v1/{}/{}/registration/{}",
                    self.options.http_protocol.as_ref().unwrap_or(&https),
                    &self.options.server_host,
                    self.options.bridge_type.as_ref().unwrap(),
                    &self.options.sender_id,
                    uaid,
                );
                let response = match Request::get(Url::parse(&url)?)
                    .headers(self.headers()?)
                    .send()
                {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(CommunicationServerError(format!(
                            "Could not fetch channel list: {}",
                            e
                        ))
                        .into());
                    }
                };
                self.check_response_error(&response)?;
                let payload: Payload = response.json()?;
                if &payload.uaid != uaid {
                    return Err(CommunicationServerError(
                        "Invalid Response from server".to_string(),
                    )
                    .into());
                }
                return Ok(payload
                    .channel_ids
                    .iter()
                    .map(|s| Store::normalize_uuid(&s))
                    .collect());
            }
            ConnectionState::NeedsReconnection => Err(ErrorKind::ReconnectError.into()),
            ConnectionState::NotConnected => {
                Err(CommunicationError("Client not connected".to_owned()).into())
            }
        }
    }

    // Add one or more new broadcast subscriptions.
    fn broadcast_subscribe(&self, _broadcast: BroadcastValue) -> error::Result<BroadcastValue> {
        Err(CommunicationError("Unsupported".to_string()).into())
    }

    // get the list of broadcasts
    fn broadcasts(&self) -> error::Result<BroadcastValue> {
        Err(CommunicationError("Unsupported".to_string()).into())
    }

    /// Verify that the server and client both have matching channel information. A "false"
    /// should force the client to drop the old UAID, request a new UAID from the server, and
    /// resubscribe all channels, resulting in new endpoints.
    fn verify_connection(&mut self, channels: &[String]) -> error::Result<bool> {
        if &self.options.sender_id == "test" {
            return Ok(false);
        }

        match &self.state {
            ConnectionState::Connected { .. } => {
                let local_channels: HashSet<String> = channels.iter().cloned().collect();
                let remote_channels: HashSet<String> = match self.channel_list() {
                    Ok(v) => HashSet::from_iter(v),
                    Err(e) => match e.kind() {
                        UAIDNotRecognizedError(_) => {
                            // We do not unsubscribe, because the
                            // server already lost our UAID
                            self.state = ConnectionState::NeedsReconnection;
                            return Ok(false);
                        }
                        _ => return Err(e),
                    },
                };

                // verify both lists match. Either side could have lost it's mind.
                if remote_channels != local_channels {
                    // Unsubscribe all the channels (just to be sure and avoid a loop).
                    self.unsubscribe_all()?;
                    return Ok(false);
                }
                Ok(true)
            }
            ConnectionState::NeedsReconnection => Err(ErrorKind::ReconnectError.into()),
            ConnectionState::NotConnected => {
                Err(CommunicationError("Client not connected".to_owned()).into())
            }
        }
    }

    //impl TODO: Handle a Ping response with updated Broadcasts.
    //impl TODO: Handle an incoming Notification
}

#[cfg(test)]
mod test {
    use super::*;

    use super::Connection;

    use mockito::{mock, server_address};
    use serde_json::json;

    const DUMMY_CHID: &str = "deadbeef00000000decafbad00000000";
    const DUMMY_CHID2: &str = "deadbeef00000000decafbad00000001";
    const DUMMY_UAID: &str = "abad1dea00000000aabbccdd00000000";
    const DUMMY_UAID2: &str = "abad1dea00000000aabbccdd00000001";

    // Local test SENDER_ID ("test*" reserved for Kotlin testing.)
    const SENDER_ID: &str = "FakeSenderID";
    const SECRET: &str = "SuP3rS1kRet";
    const SECRET2: &str = "S1kRetC0dE";

    #[test]
    fn test_communications() {
        // FIXME: this test shouldn't make network requests.
        viaduct_reqwest::use_reqwest_backend();
        // mockito forces task serialization, so for now, we test everything in one go.
        let config = PushConfiguration {
            http_protocol: Some("http".to_owned()),
            server_host: server_address().to_string(),
            sender_id: SENDER_ID.to_owned(),
            bridge_type: Some("test".to_owned()),
            registration_id: Some("SomeRegistrationValue".to_owned()),
            ..Default::default()
        };
        // SUBSCRIPTION with secret
        {
            let body = json!({
                "uaid": DUMMY_UAID,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": SECRET,
            })
            .to_string();
            let ap_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(config.clone(), ConnectionState::NotConnected).unwrap();
            let channel_id = hex::encode(crate::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            if let ConnectionState::Connected { auth, .. } = conn.state {
                assert_eq!(auth.unwrap(), SECRET.to_owned())
            } else {
                panic!("Expected connection to be established")
            }
        }
        // SUBSCRIPTION with no secret
        {
            let body = json!({
                "uaid": DUMMY_UAID,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": null,
            })
            .to_string();
            let ap_ns_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(config.clone(), ConnectionState::NotConnected).unwrap();
            let channel_id = hex::encode(crate::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_ns_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            if let ConnectionState::Connected { auth, .. } = conn.state {
                assert!(auth.is_none())
            } else {
                panic!("Expected connection to be established")
            }
        }
        // UNSUBSCRIBE - Single channel
        {
            let ap_mock = mock(
                "DELETE",
                format!(
                    "/v1/test/{}/registration/{}/subscription/{}",
                    SENDER_ID, DUMMY_UAID, DUMMY_CHID
                )
                .as_ref(),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let state = ConnectionState::Connected {
                uaid: DUMMY_UAID.to_owned(),
                auth: Some(SECRET.to_owned()),
            };
            let conn = connect(config.clone(), state).unwrap();
            let response = conn.unsubscribe(DUMMY_CHID).unwrap();
            ap_mock.assert();
            assert!(response);
        }
        // UNSUBSCRIBE - All for UAID
        {
            let ap_mock = mock(
                "DELETE",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let state = ConnectionState::Connected {
                uaid: DUMMY_UAID.to_owned(),
                auth: Some(SECRET.to_owned()),
            };
            let mut conn = connect(config.clone(), state).unwrap();
            //TODO: Add record to nuke.
            let response = conn.unsubscribe_all().unwrap();
            ap_mock.assert();
            assert!(response);
        }
        // UPDATE
        {
            let ap_mock = mock(
                "PUT",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let state = ConnectionState::Connected {
                uaid: DUMMY_UAID.to_owned(),
                auth: Some(SECRET.to_owned()),
            };
            let mut conn = connect(config.clone(), state).unwrap();

            let response = conn.update("NewTokenValue").unwrap();
            ap_mock.assert();
            assert_eq!(
                conn.options.registration_id,
                Some("NewTokenValue".to_owned())
            );
            assert!(response);
        }
        // CHANNEL LIST
        {
            let body_cl_success = json!({
                "uaid": DUMMY_UAID,
                "channelIDs": [DUMMY_CHID],
            })
            .to_string();
            let ap_mock = mock(
                "GET",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_cl_success)
            .create();
            let state = ConnectionState::Connected {
                uaid: DUMMY_UAID.to_owned(),
                auth: Some(SECRET.to_owned()),
            };
            let conn = connect(config, state).unwrap();
            let response = conn.channel_list().unwrap();
            ap_mock.assert();
            assert!(response == [DUMMY_CHID.to_owned()]);
        }

        // Test that if we failed to verify connections, we
        // wipe the uaid's from both server and locally
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
                registration_id: Some("SomeRegistrationValue".to_owned()),
                ..Default::default()
            };
            // We first subscribe to get a UAID
            let body = json!({
                "uaid": DUMMY_UAID,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": SECRET,
            })
            .to_string();
            let ap_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(config, ConnectionState::NotConnected).unwrap();
            let channel_id = hex::encode(crate::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            if let ConnectionState::Connected { auth, .. } = &conn.state {
                assert_eq!(auth.as_ref().unwrap(), &SECRET.to_owned())
            } else {
                panic!("Expected connection to be established")
            }
            // We then try to verify connection and get a mismatch
            let channel_list_body = json!({
                "uaid": DUMMY_UAID,
                "channelIDs": [DUMMY_CHID, DUMMY_CHID2],
            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            let delete_uaid_mock = mock(
                "DELETE",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            // Before we get the mismatch, we expect that we are currently connected
            assert!(matches!(conn.state, ConnectionState::Connected { .. }));
            conn.verify_connection(&[DUMMY_CHID.into()]).unwrap();
            // we verify that we got the list of channels from the server
            channel_list_mock.assert();
            // we verify that we wiped the UAID from the server
            delete_uaid_mock.assert();
            // after the mismatch, we unsubscribed, thus we expected that we now
            // are in a `NeedsReconnect` state
            assert!(matches!(conn.state, ConnectionState::NeedsReconnection));
            // we now test that when we send a new subscribe
            // we'll store the new UAID the server gets us
            let body = json!({
                "uaid": DUMMY_UAID2,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": SECRET2,
            })
            .to_string();
            let ap_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            // we verify that the UAID is the new one we got from
            // the server
            if let ConnectionState::Connected { uaid, auth } = &conn.state {
                assert_eq!(uaid, DUMMY_UAID2);
                assert_eq!(auth.as_ref().unwrap(), SECRET2);
            } else {
                panic!("Expected to be reconnected");
            }
        }
        // We test that the client detects that the server lost its
        // UAID, and verify_connection doesn't return an error
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
                registration_id: Some("SomeRegistrationValue".to_owned()),
                ..Default::default()
            };
            // We first subscribe to get a UAID
            let body = json!({
                "uaid": DUMMY_UAID,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": SECRET,
            })
            .to_string();
            let ap_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(config, ConnectionState::NotConnected).unwrap();
            let channel_id = hex::encode(crate::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            if let ConnectionState::Connected { uaid, auth } = &conn.state {
                assert_eq!(&response.uaid, uaid);
                assert_eq!(&response.secret.unwrap(), auth.as_ref().unwrap());
            } else {
                panic!("Expected to be connected");
            }
            // We mock that the server lost our UAID
            // and returns a client error
            let channel_list_body = json!({
                "code": status_codes::GONE,
                "errno": UAID_NOT_FOUND_ERRNO,
                "error": "",
                "message": "UAID not found"

            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .with_status(status_codes::GONE as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            // we verify that the call to `verify_connection` didn't error out
            // and instead is instructing its caller to wipe the UAID from
            // persisted storage
            let is_ok = conn.verify_connection(&[channel_id]).unwrap();
            assert!(!is_ok);
            channel_list_mock.assert();
            assert!(matches!(conn.state, ConnectionState::NeedsReconnection));
        }

        // We test what happens when the server responds with a different client error
        // than losing the UAID
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
                registration_id: Some("SomeRegistrationValue".to_owned()),
                ..Default::default()
            };
            // We first subscribe to get a UAID
            let body = json!({
                "uaid": DUMMY_UAID,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": SECRET,
            })
            .to_string();
            let ap_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(config, ConnectionState::NotConnected).unwrap();
            let channel_id = hex::encode(crate::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            if let ConnectionState::Connected { uaid, auth } = &conn.state {
                assert_eq!(&response.uaid, uaid);
                assert_eq!(&response.secret.unwrap(), auth.as_ref().unwrap());
            } else {
                panic!("Expected to be connected");
            }
            // We mock that the server is returning a client error
            let channel_list_body = json!({
                "code": status_codes::UNAUTHORIZED,
                "errno": 109,
                "error": "",
                "message": "Unauthroized"

            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .with_status(status_codes::UNAUTHORIZED as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            // we verify that the verify connection call will error out with a
            // communication error
            let err = conn.verify_connection(&[channel_id]).unwrap_err();
            channel_list_mock.assert();
            assert!(matches!(
                err.kind(),
                error::ErrorKind::CommunicationError(_)
            ));
            // we double check that we did not wipe our uaid
            assert!(matches!(conn.state, ConnectionState::Connected { .. }));
        }

        // We test what happens when the server responds with a server error
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
                registration_id: Some("SomeRegistrationValue".to_owned()),
                ..Default::default()
            };
            // We first subscribe to get a UAID
            let body = json!({
                "uaid": DUMMY_UAID,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": SECRET,
            })
            .to_string();
            let ap_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(config, ConnectionState::NotConnected).unwrap();
            let channel_id = hex::encode(crate::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            if let ConnectionState::Connected { uaid, auth } = &conn.state {
                assert_eq!(&response.uaid, uaid);
                assert_eq!(&response.secret.unwrap(), auth.as_ref().unwrap());
            } else {
                panic!("Expected to be connected");
            }
            // We mock that the server is returning a client error
            let channel_list_body = json!({
                "code": status_codes::INTERNAL_SERVER_ERROR,
                "errno": 999,
                "error": "",
                "message": "Unknown Error"

            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .with_status(status_codes::INTERNAL_SERVER_ERROR as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();

            // we verify that the verify connection call will error out with a
            // server error
            let err = conn.verify_connection(&[channel_id]).unwrap_err();
            channel_list_mock.assert();
            assert!(matches!(
                err.kind(),
                error::ErrorKind::CommunicationServerError(_)
            ));
            // we double check that we did not wipe our uaid
            assert!(matches!(conn.state, ConnectionState::Connected { .. }));
        }

        // we test that we properly return a `AlreadyRegisteredError` when a client
        // gets a `CONFLICT` status code
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
                registration_id: Some("SomeRegistrationValue".to_owned()),
                ..Default::default()
            };
            // We mock that the server thinks
            // we already registered!
            let body = json!({
                "code": status_codes::CONFLICT,
                "errno": 999,
                "error": "",
                "message": "Already registered"

            })
            .to_string();
            let ap_mock = mock(
                "POST",
                format!("/v1/test/{}/registration", SENDER_ID).as_ref(),
            )
            .with_status(status_codes::CONFLICT as usize)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(config, ConnectionState::NotConnected).unwrap();
            let channel_id = hex::encode(crate::crypto::get_random_bytes(16).unwrap());
            let err = conn.subscribe(&channel_id, None).unwrap_err();
            ap_mock.assert();
            assert!(matches!(
                err.kind(),
                error::ErrorKind::AlreadyRegisteredError
            ));
        }
    }
}
