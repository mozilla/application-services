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
#[derive(Deserialize, Debug)]
pub struct RegisterResponse {
    /// The UAID associated with the request
    pub uaid: Option<String>,

    /// The Channel ID associated with the request
    #[serde(rename = "channelID")]
    pub channel_id: String,

    /// Auth token for subsequent calls (note, only generated on new UAIDs)
    pub secret: Option<String>,

    /// Push endpoint for 3rd parties
    pub endpoint: String,

    /// The sender id
    #[serde(rename = "senderid")]
    pub sender_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum BroadcastValue {
    Value(String),
    Nested(HashMap<String, BroadcastValue>),
}
/// A new communication link to the Autopush server
#[cfg_attr(test, mockall::automock)]
pub trait Connection: Sized {
    /// Create a new instance of a [`Connection`]
    fn connect(options: PushConfiguration) -> Self;

    /// send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &self,
        channel_id: &str,
        uaid: &Option<String>,
        auth: &Option<String>,
        registration_id: &str,
        app_server_key: &Option<String>,
    ) -> error::Result<RegisterResponse>;

    /// Drop an endpoint
    fn unsubscribe(&self, channel_id: &str, uaid: &str, auth: &str) -> error::Result<()>;

    /// drop all endpoints
    fn unsubscribe_all(&self, uaid: &str, auth: &str) -> error::Result<()>;

    /// Update the autopush server with the new native OS Messaging authorization token
    fn update(&self, new_token: &str, uaid: &str, auth: &str) -> error::Result<()>;

    /// Get a list of server known channels.
    fn channel_list(&self, uaid: &str, auth: &str) -> error::Result<Vec<String>>;

    /// Verify that the known channel list matches up with the server list. If this fails, regenerate endpoints.
    /// This should be performed once a day.
    fn verify_connection(&self, channels: &[String], uaid: &str, auth: &str)
        -> error::Result<bool>;
}

/// Connect to the Autopush server via the HTTP interface
pub struct ConnectHttp {
    options: PushConfiguration, // Server auth token
}

impl ConnectHttp {
    fn auth_headers(&self, auth: &str) -> error::Result<Headers> {
        let mut headers = Headers::new();
        headers
            .insert(header_names::AUTHORIZATION, &*format!("webpush {}", auth))
            .map_err(|e| error::PushError::CommunicationError(format!("Header error: {:?}", e)))?;

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

    fn format_unsubscribe_url(&self, uaid: &str) -> error::Result<String> {
        Ok(format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &self.options.http_protocol,
            &self.options.server_host,
            &self.options.bridge_type,
            &self.options.sender_id,
            &uaid,
        ))
    }
}

impl Connection for ConnectHttp {
    // Connect to the Autopush server
    fn connect(options: PushConfiguration) -> ConnectHttp {
        ConnectHttp { options }
    }

    /// send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &self,
        channel_id: &str,
        uaid: &Option<String>,
        auth: &Option<String>,
        registration_id: &str,
        app_server_key: &Option<String>,
    ) -> error::Result<RegisterResponse> {
        let mut url = format!(
            "{}://{}/v1/{}/{}/registration",
            &self.options.http_protocol,
            &self.options.server_host,
            &self.options.bridge_type,
            &self.options.sender_id
        );
        // Add the uaid and authorization if we have a prior subscription.
        if let Some(uaid) = uaid {
            url.push('/');
            url.push_str(uaid);
            url.push_str("/subscription");
        }

        let headers = if let Some(auth) = auth {
            self.auth_headers(auth)?
        } else {
            Headers::new()
        };

        let mut body = HashMap::new();
        // Ideally we'd store "expected" subscriptions in the DB separate from "actual" ones, and
        // then we could record this as "expected" and perform the actual subscription once we
        // learn our registration_id - but for now we can't do anything subscription related
        // without a registration_id.
        body.insert("token", registration_id);
        body.insert("channelID", channel_id);
        if let Some(key) = app_server_key {
            body.insert("key", key);
        }
        let response = Request::post(Url::parse(&url)?)
            .headers(headers)
            .json(&body)
            .send()?;
        log::info!(
            "subscribed to channel '{}' via {:?} - {}",
            channel_id,
            url,
            response.status
        );
        self.check_response_error(&response)?;
        Ok(response.json()?)
    }

    /// Drop a channel and stop receiving updates.
    fn unsubscribe(&self, channel_id: &str, uaid: &str, auth: &str) -> error::Result<()> {
        let url = format!(
            "{}/subscription/{}",
            self.format_unsubscribe_url(uaid)?,
            channel_id
        );
        let response = Request::delete(Url::parse(&url)?)
            .headers(self.auth_headers(auth)?)
            .send()?;
        log::info!("unsubscribed from {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
    }

    /// Drops all channels and stops receiving notifications.
    fn unsubscribe_all(&self, uaid: &str, auth: &str) -> error::Result<()> {
        let url = self.format_unsubscribe_url(uaid)?;
        let response = Request::delete(Url::parse(&url)?)
            .headers(self.auth_headers(auth)?)
            .send()?;
        log::info!("unsubscribed from all via {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
    }

    /// Update the push server with the new OS push authorization token
    fn update(&self, new_token: &str, uaid: &str, auth: &str) -> error::Result<()> {
        let options = self.options.clone();
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &options.http_protocol,
            &options.server_host,
            &options.bridge_type,
            &options.sender_id,
            uaid
        );
        let mut body = HashMap::new();
        body.insert("token", new_token);
        let response = Request::put(Url::parse(&url)?)
            .json(&body)
            .headers(self.auth_headers(auth)?)
            .send()?;
        log::info!("update via {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
    }

    /// Get a list of server known channels. If it differs from what we have, reset the UAID, and refresh channels.
    /// Should be done once a day.
    fn channel_list(&self, uaid: &str, auth: &str) -> error::Result<Vec<String>> {
        #[derive(Deserialize, Debug)]
        struct Payload {
            uaid: String,
            #[serde(rename = "channelIDs")]
            channel_ids: Vec<String>,
        }

        let options = self.options.clone();

        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &options.http_protocol,
            &options.server_host,
            &options.bridge_type,
            &options.sender_id,
            &uaid,
        );
        let response = match Request::get(Url::parse(&url)?)
            .headers(self.auth_headers(auth)?)
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
        if payload.uaid != uaid {
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
    /// Verify that the server and client both have matching channel information. A "false"
    /// should force the client to drop the old UAID, request a new UAID from the server, and
    /// resubscribe all channels, resulting in new endpoints.
    fn verify_connection(
        &self,
        channels: &[String],
        uaid: &str,
        auth: &str,
    ) -> error::Result<bool> {
        let local_channels: HashSet<String> = channels.iter().cloned().collect();
        let remote_channels: HashSet<String> = match self.channel_list(uaid, auth) {
            Ok(v) => HashSet::from_iter(v),
            Err(e) => match e {
                UAIDNotRecognizedError(_) => {
                    // We do not unsubscribe, because the server already lost our UAID
                    return Ok(false);
                }
                _ => return Err(e),
            },
        };

        // verify both lists match. Either side could have lost its mind.
        if remote_channels != local_channels {
            log::info!("verify_connection found a mismatch - unsubscribing");
            // Unsubscribe all the channels (just to be sure and avoid a loop).
            self.unsubscribe_all(uaid, auth)?;
            return Ok(false);
        }
        log::info!("verify_connection found everything matching");
        Ok(true)
    }
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
            http_protocol: "http".to_owned(),
            server_host: server_address().to_string(),
            sender_id: SENDER_ID.to_owned(),
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
            let ap_mock = mock("POST", &*format!("/v1/fcm/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let conn = ConnectHttp::connect(config.clone());
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn
                .subscribe(&channel_id, &None, &None, SENDER_ID, &None)
                .unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, Some(DUMMY_UAID.to_string()));
        }
        // UNSUBSCRIBE - Single channel
        {
            let ap_mock = mock(
                "DELETE",
                &*format!(
                    "/v1/fcm/{}/registration/{}/subscription/{}",
                    SENDER_ID, DUMMY_UAID, DUMMY_CHID
                ),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let conn = ConnectHttp::connect(config.clone());
            conn.unsubscribe(DUMMY_CHID, DUMMY_UAID, SECRET).unwrap();
            ap_mock.assert();
        }
        // UNSUBSCRIBE - All for UAID
        {
            let ap_mock = mock(
                "DELETE",
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let conn = ConnectHttp::connect(config.clone());
            conn.unsubscribe_all(DUMMY_UAID, SECRET).unwrap();
            ap_mock.assert();
        }
        // UPDATE
        {
            let ap_mock = mock(
                "PUT",
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let conn = ConnectHttp::connect(config.clone());

            conn.update("NewTokenValue", DUMMY_UAID, SECRET).unwrap();
            ap_mock.assert();
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
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_cl_success)
            .create();
            let conn = ConnectHttp::connect(config);
            let response = conn.channel_list(DUMMY_UAID, SECRET).unwrap();
            ap_mock.assert();
            assert!(response == [DUMMY_CHID.to_owned()]);
        }

        // Test that if we failed to verify connections, we
        // wipe the uaid's from both server and locally
        {
            let config = PushConfiguration {
                http_protocol: "http".to_owned(),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
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
            let ap_mock = mock("POST", &*format!("/v1/fcm/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let conn = ConnectHttp::connect(config);
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn
                .subscribe(&channel_id, &None, &None, SENDER_ID, &None)
                .unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, Some(DUMMY_UAID.to_string()));
            // We then try to verify connection and get a mismatch
            let channel_list_body = json!({
                "uaid": DUMMY_UAID,
                "channelIDs": [DUMMY_CHID, DUMMY_CHID2],
            })
            .to_string();
            let channel_list_mock = mock(
                "GET",
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            let delete_uaid_mock = mock(
                "DELETE",
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .match_header("authorization", format!("webpush {}", SECRET).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            conn.verify_connection(&[DUMMY_CHID.into()], DUMMY_UAID, SECRET)
                .unwrap();
            // we verify that we got the list of channels from the server
            channel_list_mock.assert();
            // we verify that we wiped the UAID from the server
            delete_uaid_mock.assert();

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
            let ap_mock = mock("POST", &*format!("/v1/fcm/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let res = conn
                .subscribe(&channel_id, &None, &None, SENDER_ID, &None)
                .unwrap();
            ap_mock.assert();
            // we verify that the UAID is the new one we got from
            // the server
            assert_eq!(res.uaid, Some(DUMMY_UAID2.to_string()));
            assert_eq!(res.secret, Some(SECRET2.to_string()));
        }
        // We test that the client detects that the server lost its
        // UAID, and verify_connection doesn't return an error
        {
            let config = PushConfiguration {
                http_protocol: "http".to_owned(),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
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
            let ap_mock = mock("POST", &*format!("/v1/fcm/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let conn = ConnectHttp::connect(config);
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn
                .subscribe(&channel_id, &None, &None, SENDER_ID, &None)
                .unwrap();
            ap_mock.assert();

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
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .with_status(status_codes::GONE as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            // we verify that the call to `verify_connection` didn't error out
            // and instead is instructing its caller to wipe the UAID from
            // persisted storage
            let is_ok = conn
                .verify_connection(
                    &[channel_id],
                    &response.uaid.unwrap(),
                    &response.secret.unwrap(),
                )
                .unwrap();
            assert!(!is_ok);
            channel_list_mock.assert();
        }

        // We test what happens when the server responds with a different client error
        // than losing the UAID
        {
            let config = PushConfiguration {
                http_protocol: "http".to_owned(),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
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
            let ap_mock = mock("POST", &*format!("/v1/fcm/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let conn = ConnectHttp::connect(config);
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn
                .subscribe(&channel_id, &None, &None, SENDER_ID, &None)
                .unwrap();
            ap_mock.assert();

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
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .with_status(status_codes::UNAUTHORIZED as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();
            // we verify that the verify connection call will error out with a
            // communication error
            let err = conn
                .verify_connection(
                    &[channel_id],
                    &response.uaid.unwrap(),
                    &response.secret.unwrap(),
                )
                .unwrap_err();
            channel_list_mock.assert();
            assert!(matches!(err, error::PushError::CommunicationError(_)));
        }

        // We test what happens when the server responds with a server error
        {
            let config = PushConfiguration {
                http_protocol: "http".to_owned(),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
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
            let ap_mock = mock("POST", &*format!("/v1/fcm/{}/registration", SENDER_ID))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let conn = ConnectHttp::connect(config);
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let response = conn
                .subscribe(&channel_id, &None, &None, SENDER_ID, &None)
                .unwrap();
            ap_mock.assert();
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
                &*format!("/v1/fcm/{}/registration/{}", SENDER_ID, DUMMY_UAID),
            )
            .with_status(status_codes::INTERNAL_SERVER_ERROR as usize)
            .with_header("content-type", "application/json")
            .with_body(channel_list_body)
            .create();

            // we verify that the verify connection call will error out with a
            // server error
            let err = conn
                .verify_connection(
                    &[channel_id],
                    &response.uaid.unwrap(),
                    &response.secret.unwrap(),
                )
                .unwrap_err();
            channel_list_mock.assert();
            assert!(matches!(err, error::PushError::CommunicationServerError(_)));
        }

        // we test that we properly return a `AlreadyRegisteredError` when a client
        // gets a `CONFLICT` status code
        {
            let config = PushConfiguration {
                http_protocol: "http".to_owned(),
                server_host: server_address().to_string(),
                sender_id: SENDER_ID.to_owned(),
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
            let ap_mock = mock("POST", &*format!("/v1/fcm/{}/registration", SENDER_ID))
                .with_status(status_codes::CONFLICT as usize)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            let conn = ConnectHttp::connect(config);
            let channel_id = hex::encode(crate::internal::crypto::get_random_bytes(16).unwrap());
            let err = conn
                .subscribe(&channel_id, &None, &None, SENDER_ID, &None)
                .unwrap_err();
            ap_mock.assert();
            assert!(matches!(err, error::PushError::AlreadyRegisteredError));
        }
    }
}
