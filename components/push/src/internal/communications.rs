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

use crate::error::{
    self,
    PushError::{
        AlreadyRegisteredError, CommunicationError, CommunicationServerError,
        UAIDNotRecognizedError,
    },
};
use crate::internal::config::PushConfiguration;
use crate::internal::storage::Store;

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
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum BroadcastValue {
    Value(String),
    Nested(HashMap<String, BroadcastValue>),
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
    fn unsubscribe(&self, channel_id: &str) -> error::Result<()>;

    /// drop all endpoints
    fn unsubscribe_all(&mut self) -> error::Result<()>;

    /// Update the autopush server with the new native OS Messaging authorization token
    fn update(&mut self, new_token: &str) -> error::Result<()>;

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
    pub uaid: Option<String>,
    pub auth: Option<String>,            // Server auth token
    pub registration_id: Option<String>, // Eg, the FCM/iOS native ID
}

// Connect to the Autopush server
pub fn connect(
    options: PushConfiguration,
    uaid: Option<String>,
    auth: Option<String>,
    registration_id: Option<String>,
) -> error::Result<ConnectHttp> {
    // find connection via options

    if options.socket_protocol.is_some() && options.http_protocol.is_some() {
        return Err(CommunicationError(
            "Both socket and HTTP protocols cannot be set.".to_owned(),
        ));
    };
    if options.socket_protocol.is_some() {
        return Err(CommunicationError("Unsupported".to_owned()));
    };
    let connection = ConnectHttp {
        options,
        uaid,
        auth,
        registration_id,
    };
    Ok(connection)
}

impl ConnectHttp {
    fn headers(&self) -> error::Result<Headers> {
        let mut headers = Headers::new();
        if self.auth.is_some() {
            headers
                .insert(
                    header_names::AUTHORIZATION,
                    &*format!("webpush {}", self.auth.clone().unwrap()),
                )
                .map_err(|e| {
                    error::PushError::CommunicationError(format!("Header error: {:?}", e))
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
            )));
        }
        if response.is_client_error() {
            let response_error = response.json::<ResponseError>()?;
            if response.status == status_codes::CONFLICT {
                return Err(AlreadyRegisteredError);
            }
            if response.status == status_codes::GONE && response_error.errno == UAID_NOT_FOUND_ERRNO
            {
                return Err(UAIDNotRecognizedError(response_error.message));
            }
            return Err(CommunicationError(format!(
                "Unhandled client error {:?}",
                response
            )));
        }
        Ok(())
    }

    fn format_unsubscribe_url(&self) -> error::Result<String> {
        let uaid = match &self.uaid {
            Some(u) => u,
            _ => return Err(CommunicationError("No UAID set".into())),
        };
        Ok(format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &self.options.http_protocol.as_ref().unwrap(),
            &self.options.server_host,
            &self.options.bridge_type.as_ref().unwrap(),
            &self.options.sender_id,
            &uaid,
        ))
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
            return Err(CommunicationError(
                "Bridge type or application id not set.".to_owned(),
            ));
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
        if let Some(uaid) = &self.uaid {
            url.push('/');
            url.push_str(uaid);
            url.push_str("/subscription");
        }
        let mut body = HashMap::new();
        // Ideally we'd store "expected" subscriptions in the DB separate from "actual" ones, and
        // then we could record this as "expected" and perform the actual subscription once we
        // learn our registration_id - but for now we can't do anything subscription related
        // without a registration_id.
        body.insert(
            "token",
            match &self.registration_id {
                Some(r) => r.to_owned(),
                None => {
                    return Err(CommunicationError(
                        "Can't subscribe until we have a native registration id".to_string(),
                    ))
                }
            },
        );
        body.insert("channelID", channel_id.to_owned());
        if let Some(key) = app_server_key {
            body.insert("key", key.to_owned());
        }
        // for unit tests, we shouldn't call the server. This is because we would need to create
        // a valid FCM test senderid (and make sure we call it), etc. There has also been a
        // history of problems where doing this tends to fail because of uncontrolled, third party
        // system reliability issues making the tree turn orange.
        if &self.options.sender_id == "test" {
            self.uaid = Some("abad1d3a00000000aabbccdd00000000".to_owned());
            self.auth = Some("LsuUOBKVQRY6-l7_Ajo-Ag".to_owned());

            return Ok(RegisterResponse {
                uaid: self.uaid.clone().unwrap(),
                channel_id: "deadbeef00000000decafbad00000000".to_owned(),
                secret: self.auth.clone(),
                endpoint: "http://push.example.com/test/opaque".to_owned(),
            });
        }
        let response = Request::post(Url::parse(&url)?)
            .headers(self.headers()?)
            .json(&body)
            .send()?;
        log::info!(
            "subscribed to channel '{}' via {:?} - {}",
            channel_id,
            url,
            response.status
        );
        self.check_response_error(&response)?;
        let response: Value = response.json()?;
        // asserting here seems bad! :) But what does this mean? We supplied ours, how could
        // the server disagree? (The server seems to only supply this if the UAID changed?)
        if let Some(sid) = response["senderid"].as_str() {
            assert_eq!(sid, options.sender_id, "`senderid` is confused?");
        }

        // helper to force "mandatory" fields in the response.
        let ensure_resp_field = |name: &str| -> error::Result<String> {
            match response[name].as_str() {
                Some(s) => Ok(s.to_string()),
                None => Err(CommunicationError(format!("response has no `{}`", name))),
            }
        };

        // In practice, we seem to be getting response from the server
        // without the `uaid` field, so we attempt to use the `uaid`
        // we have cached if that exists.
        // look at: https://github.com/mozilla/application-services/issues/4691
        let uaid = match response["uaid"].as_str() {
            Some(s) => s.to_string(),
            None => {
                log::warn!("Server did not return a uaid");
                if let Some(uaid) = &self.uaid {
                    log::info!("Old uaid exists, using that: {}", uaid);
                    uaid.clone()
                } else {
                    return Err(CommunicationError("Could not determine uaid".into()));
                }
            }
        };
        // secret only returned when uaid changes.
        let secret = response["secret"].as_str().map(ToString::to_string);
        // XXX - we only update `self.` here due to tests. We should fix the tests, and while at
        // it, drop the `&mut self` everywhere, to further force the requirement that it be short
        // lived.
        self.uaid = Some(uaid.clone());
        self.auth = secret.clone();
        Ok(RegisterResponse {
            uaid,
            secret,
            channel_id: ensure_resp_field("channelID")?,
            endpoint: ensure_resp_field("endpoint")?,
        })
    }

    /// Drop a channel and stop receiving updates.
    fn unsubscribe(&self, channel_id: &str) -> error::Result<()> {
        if &self.options.sender_id == "test" {
            return Ok(());
        }
        let url = format!(
            "{}/subscription/{}",
            self.format_unsubscribe_url()?,
            channel_id
        );
        let response = Request::delete(Url::parse(&url)?)
            .headers(self.headers()?)
            .send()?;
        log::info!("unsubscribed from {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
    }

    /// Drops all channels and stops receiving notifications.
    fn unsubscribe_all(&mut self) -> error::Result<()> {
        if &self.options.sender_id == "test" {
            return Ok(());
        }
        let url = self.format_unsubscribe_url()?;
        let response = Request::delete(Url::parse(&url)?)
            .headers(self.headers()?)
            .send()?;
        log::info!("unsubscribed from all via {}: {}", url, response.status);
        self.check_response_error(&response)?;
        // theoretically no need to kill our uaid etc here - this connection is short-lived, and
        // our caller is what must kill the persisted version - but tests still use it.
        self.uaid = None;
        self.auth = None;
        Ok(())
    }

    /// Update the push server with the new OS push authorization token
    fn update(&mut self, new_token: &str) -> error::Result<()> {
        if self.options.sender_id == "test" {
            self.uaid = Some("abad1d3a00000000aabbccdd00000000".to_owned());
            self.auth = Some("LsuUOBKVQRY6-l7_Ajo-Ag".to_owned());
            return Ok(());
        }
        let uaid = match &self.uaid {
            Some(u) => u,
            _ => return Err(CommunicationError("No UAID set".into())),
        };
        // Updating `self.registration_id` shouldn't be necessary - `self` should not live beyond
        // this call and it's our caller who persists the new value and supplies it.
        self.registration_id = Some(new_token.to_string());
        let options = self.options.clone();
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &options.http_protocol.unwrap(),
            &options.server_host,
            &options.bridge_type.unwrap(),
            &options.sender_id,
            uaid
        );
        let mut body = HashMap::new();
        body.insert("token", new_token);
        let response = Request::put(Url::parse(&url)?)
            .json(&body)
            .headers(self.headers()?)
            .send()?;
        log::info!("update via {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
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

        if self.auth.is_none() {
            return Err(CommunicationError("Connection is unauthorized".into()));
        }
        if self.uaid.is_none() {
            return Err(CommunicationError("No UAID set".into()));
        }
        let options = self.options.clone();
        if options.bridge_type.is_none() {
            return Err(CommunicationError("No Bridge Type set".into()));
        }
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &options.http_protocol.unwrap_or_else(|| "https".to_owned()),
            &options.server_host,
            &options.bridge_type.unwrap(),
            &options.sender_id,
            &self.uaid.clone().unwrap(),
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
                )));
            }
        };
        self.check_response_error(&response)?;
        let payload: Payload = response.json()?;
        if payload.uaid != self.uaid.clone().unwrap() {
            return Err(CommunicationServerError(
                "Invalid Response from server".to_string(),
            ));
        }
        Ok(payload
            .channel_ids
            .iter()
            .map(|s| Store::normalize_uuid(s))
            .collect())
    }

    // Add one or more new broadcast subscriptions.
    fn broadcast_subscribe(&self, _broadcast: BroadcastValue) -> error::Result<BroadcastValue> {
        Err(CommunicationError("Unsupported".to_string()))
    }

    // get the list of broadcasts
    fn broadcasts(&self) -> error::Result<BroadcastValue> {
        Err(CommunicationError("Unsupported".to_string()))
    }

    /// Verify that the server and client both have matching channel information. A "false"
    /// should force the client to drop the old UAID, request a new UAID from the server, and
    /// resubscribe all channels, resulting in new endpoints.
    fn verify_connection(&mut self, channels: &[String]) -> error::Result<bool> {
        if &self.options.sender_id == "test" {
            return Ok(false);
        }
        let local_channels: HashSet<String> = channels.iter().cloned().collect();
        let remote_channels: HashSet<String> = match self.channel_list() {
            Ok(v) => HashSet::from_iter(v),
            Err(e) => match e {
                UAIDNotRecognizedError(_) => {
                    // We do not unsubscribe, because the server already lost our UAID
                    // XXX - update `self` just for tests. Should kill `&mut self`
                    self.uaid = None;
                    self.auth = None;
                    return Ok(false);
                }
                _ => return Err(e),
            },
        };

        // verify both lists match. Either side could have lost its mind.
        if remote_channels != local_channels {
            log::info!("verify_connection found a mismatch - unsubscribing");
            // Unsubscribe all the channels (just to be sure and avoid a loop).
            self.unsubscribe_all()?;
            return Ok(false);
        }
        log::info!("verify_connection found everything matching");
        Ok(true)
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
            let ap_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let mut conn =
                connect(config.clone(), None, None, Some(SENDER_ID.to_string())).unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            assert_eq!(conn.auth, Some(SECRET.to_owned()));
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
            let ap_ns_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let mut conn =
                connect(config.clone(), None, None, Some(SENDER_ID.to_string())).unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_ns_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            assert_eq!(conn.auth, None);
        }
        // SUBSCRIPTION - uaid already cached, but server
        // doesn't return a uaid
        {
            let body = json!({
                "uaid": null,
                "channelID": DUMMY_CHID,
                "endpoint": "https://example.com/update",
                "senderid": SENDER_ID,
                "secret": null,
            })
            .to_string();
            let ap_ns_mock = mock(
                "POST",
                &*format!(
                    "/v1/test/{}/registration/{}/subscription",
                    SENDER_ID, DUMMY_UAID
                ),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
            let mut conn = connect(
                config.clone(),
                Some(DUMMY_UAID.into()),
                None,
                Some(SENDER_ID.to_string()),
            )
            .unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_ns_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            assert_eq!(conn.auth, None);
        }
        // UNSUBSCRIBE - Single channel
        {
            let ap_mock = mock(
                "DELETE",
                &*format!(
                    "/v1/test/{}/registration/{}/subscription/{}",
                    SENDER_ID, DUMMY_UAID, DUMMY_CHID
                ),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let conn = connect(
                config.clone(),
                Some(DUMMY_UAID.to_owned()),
                Some(SECRET.to_owned()),
                None,
            )
            .unwrap();
            conn.unsubscribe(DUMMY_CHID).unwrap();
            ap_mock.assert();
        }
        // UNSUBSCRIBE - All for UAID
        {
            let ap_mock = mock(
                "DELETE",
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let mut conn = connect(
                config.clone(),
                Some(DUMMY_UAID.to_owned()),
                Some(SECRET.to_owned()),
                None,
            )
            .unwrap();
            //TODO: Add record to nuke.
            conn.unsubscribe_all().unwrap();
            ap_mock.assert();
        }
        // UPDATE
        {
            let ap_mock = mock(
                "PUT",
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let mut conn = connect(
                config.clone(),
                Some(DUMMY_UAID.to_owned()),
                Some(SECRET.to_owned()),
                Some("native-id".to_string()),
            )
            .unwrap();

            conn.update("NewTokenValue").unwrap();
            ap_mock.assert();
            assert_eq!(conn.registration_id, Some("NewTokenValue".to_owned()));
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
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_cl_success)
            .create();
            let conn = connect(
                config,
                Some(DUMMY_UAID.to_owned()),
                Some(SECRET.to_owned()),
                None,
            )
            .unwrap();
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
            let ap_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let mut conn = connect(config, None, None, Some(SENDER_ID.to_string())).unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            assert_eq!(conn.auth, Some(SECRET.to_owned()));
            // We then try to verify connection and get a mismatch
            let channel_list_body = json!({
                "uaid": DUMMY_UAID,
                "channelIDs": [DUMMY_CHID, DUMMY_CHID2],
            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            let delete_uaid_mock = mock(
                "DELETE",
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            // Before we get the mismatch, we expect that we have a valid UAID
            assert!(conn.uaid.is_some());
            conn.verify_connection(&[DUMMY_CHID.into()]).unwrap();
            // we verify that we got the list of channels from the server
            channel_list_mock.assert();
            // we verify that we wiped the UAID from the server
            delete_uaid_mock.assert();
            // after the mismatch, we unsubscribed, thus we expect that we wiped the UAID
            // from the conn object too
            assert!(conn.uaid.is_none());
            // But the native id isn't wiped
            assert!(conn.registration_id.is_some());
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
            let ap_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            // we verify that the UAID is the new one we got from
            // the server
            assert_eq!(conn.uaid.unwrap(), DUMMY_UAID2);
            assert_eq!(conn.auth.unwrap(), SECRET2);
        }
        // We test that the client detects that the server lost its
        // UAID, and verify_connection doesn't return an error
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
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
            let ap_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let mut conn = connect(config, None, None, Some(SENDER_ID.to_string())).unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, conn.uaid.clone().unwrap());
            assert_eq!(response.secret.unwrap(), conn.auth.clone().unwrap());
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
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
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
            assert!(conn.uaid.is_none());
            assert!(conn.auth.is_none());
        }

        // We test what happens when the server responds with a different client error
        // than losing the UAID
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
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
            let ap_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let mut conn = connect(config, None, None, Some(SENDER_ID.to_string())).unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, conn.uaid.clone().unwrap());
            assert_eq!(response.secret.unwrap(), conn.auth.clone().unwrap());
            // We mock that the server is returning a client error
            let channel_list_body = json!({
                "code": status_codes::UNAUTHORIZED,
                "errno": 109u32,
                "error": "",
                "message": "Unauthroized"

            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .with_status(status_codes::UNAUTHORIZED as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            // we verify that the verify connection call will error out with a
            // communication error
            let err = conn.verify_connection(&[channel_id]).unwrap_err();
            channel_list_mock.assert();
            assert!(matches!(err, error::PushError::CommunicationError(_)));
            // we double check that we did not wipe our uaid
            assert!(conn.uaid.is_some());
            assert!(conn.auth.is_some());
        }

        // We test what happens when the server responds with a server error
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
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
            let ap_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let mut conn = connect(config, None, None, Some(SENDER_ID.to_string())).unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id, None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, conn.uaid.clone().unwrap());
            assert_eq!(response.secret.unwrap(), conn.auth.clone().unwrap());
            // We mock that the server is returning a client error
            let channel_list_body = json!({
                "code": status_codes::INTERNAL_SERVER_ERROR,
                "errno": 999u32,
                "error": "",
                "message": "Unknown Error"

            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                &*format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .with_status(status_codes::INTERNAL_SERVER_ERROR as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();

            // we verify that the verify connection call will error out with a
            // server error
            let err = conn.verify_connection(&[channel_id]).unwrap_err();
            channel_list_mock.assert();
            assert!(matches!(err, error::PushError::CommunicationServerError(_)));
            // we double check that we did not wipe our uaid
            assert!(conn.uaid.is_some());
            assert!(conn.auth.is_some());
        }

        // we test that we properly return a `AlreadyRegisteredError` when a client
        // gets a `CONFLICT` status code
        {
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
                bridge_type: Some("test".to_owned()),
                ..Default::default()
            };
            // We mock that the server thinks
            // we already registered!
            let body = json!({
                "code": status_codes::CONFLICT,
                "errno": 999u32,
                "error": "",
                "message": "Already registered"

            })
            .to_string();
            let ap_mock = mock("POST", &*format!("/v1/test/{}/registration", SENDER_ID))
                .with_status(status_codes::CONFLICT as usize)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let mut conn = connect(config, None, None, Some(SENDER_ID.to_string())).unwrap();
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let err = conn.subscribe(&channel_id, None).unwrap_err();
            ap_mock.assert();
            assert!(matches!(err, error::PushError::AlreadyRegisteredError));
        }
    }
}
