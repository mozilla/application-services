/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Server Communications.
//!
//! Handles however communication to and from the remote Push Server should be done. For Desktop
//! this will be over Websocket. For mobile, it calls into the local operating
//! system and HTTPS to the web push server.
//!
//! Mainly exposes a trait [`Connection`] and a concrete type that implements it [`ConnectHttp`]
//!
//! The trait is a lightweight interface that talks to autopush servers and provides the following functionality
//! - Subscription: Through [`Connection::subscribe_new`] on first subscription, and [`Connection::subscribe_with_uaid`] on subsequent subscriptiosn
//! - Unsubscription: Through [`Connection::unsubscribe`] for a single channel, and [`Connection::unsubscribe_all`] for all channels
//! - Updating tokens: Through [`Connection::update`] to update a native token
//! - Getting all subscription channels: Through [`Connection::channel_list`]

use serde::{Deserialize, Serialize};
use url::Url;
use viaduct::{header_names, status_codes, Headers, Request};

use crate::error::{
    self, info,
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
/// The response from the `/registration` endpoint
pub struct RegisterResponse {
    /// The UAID assigned by autopush
    pub uaid: String,

    /// The Channel ID associated with the request
    /// The server might assign a new one if "" is sent
    /// with the request. Consumers should treat this channel_id
    /// as the tru channel id.
    #[serde(rename = "channelID")]
    pub channel_id: String,

    /// Auth token for subsequent calls (note, only generated on new UAIDs)
    pub secret: String,

    /// Push endpoint for 3rd parties
    pub endpoint: String,

    /// The sender id
    #[allow(dead_code)]
    #[serde(rename = "senderid")]
    pub sender_id: Option<String>,
}

#[derive(Deserialize, Debug)]
/// The response from the `/subscribe` endpoint
pub struct SubscribeResponse {
    /// The Channel ID associated with the request
    /// The server sends it back with the response.
    /// The server might assign a new one if "" is sent
    /// with the request. Consumers should treat this channel_id
    /// as the tru channel id
    #[serde(rename = "channelID")]
    pub channel_id: String,

    /// Push endpoint for 3rd parties
    pub endpoint: String,

    /// The sender id
    #[allow(dead_code)]
    #[serde(rename = "senderid")]
    pub sender_id: Option<String>,
}

#[derive(Serialize)]
/// The request body for the `/registration` endpoint
struct RegisterRequest<'a> {
    /// The native registration id, a token provided by the app
    token: &'a str,

    /// An optional app server key
    key: Option<&'a str>,
}

#[derive(Serialize)]
struct UpdateRequest<'a> {
    token: &'a str,
}

/// A new communication link to the Autopush server
#[cfg_attr(test, mockall::automock)]
pub trait Connection: Sized {
    /// Create a new instance of a [`Connection`]
    fn connect(options: PushConfiguration) -> Self;

    /// Sends this client's very first subscription request. Note that the `uaid` is not available at this stage
    /// the server will assign and return a uaid. Subsequent subscriptions will call [`Connection::subscribe_with_uaid`]
    ///
    /// # Arguments
    /// - `registration_id`: A string representing a native token. In practice, this is a Firebase token for Android and a APNS token for iOS
    /// - `app_server_key`: Optional VAPID public key to "lock" subscriptions
    ///
    /// # Returns
    /// - Returns a [`RegisterResponse`] which is the autopush server's registration response deserialized
    fn register(
        &self,
        registration_id: &str,
        app_server_key: &Option<String>,
    ) -> error::Result<RegisterResponse>;

    /// Sends subsequent subscriptions for this client. This will be called when the client has already been assigned a `uaid`
    /// by the server when it first called [`Connection::subscribe_new`]
    /// # Arguments
    /// - `uaid`: A string representing the users `uaid` that was assigned when the user first registered for a subscription
    /// - `auth`: A string representing an authorization token that will be sent as a header to autopush. The auth was returned on the user's first subscription.
    /// - `registration_id`: A string representing a native token. In practice, this is a Firebase token for Android and a APNS token for iOS
    /// - `app_server_key`: Optional VAPID public key to "lock" subscriptions
    ///
    /// # Returns
    /// - Returns a [`RegisterResponse`] which is the autopush server's registration response deserialized
    fn subscribe(
        &self,
        uaid: &str,
        auth: &str,
        registration_id: &str,
        app_server_key: &Option<String>,
    ) -> error::Result<SubscribeResponse>;

    /// Drop a subscription previously registered with autopush
    /// # Arguments
    /// - `channel_id`: A string defined by client. The client is expected to provide this id when requesting the subscription record
    /// - `uaid`: A string representing the users `uaid` that was assigned when the user first registered for a subscription
    /// - `auth`: A string representing an authorization token that will be sent as a header to autopush. The auth was returned on the user's first subscription.
    fn unsubscribe(&self, channel_id: &str, uaid: &str, auth: &str) -> error::Result<()>;

    /// Drop all subscriptions previously registered with autopush
    /// # Arguments
    /// - `channel_id`: A string defined by client. The client is expected to provide this id when requesting the subscription record
    /// - `uaid`: A string representing the users `uaid` that was assigned when the user first registered for a subscription
    /// - `auth`: A string representing an authorization token that will be sent as a header to autopush. The auth was returned on the user's first subscription.
    fn unsubscribe_all(&self, uaid: &str, auth: &str) -> error::Result<()>;

    /// Update the autopush server with the new native OS Messaging authorization token
    /// # Arguments
    /// - `new_token`: A string representing a new natvie token for the user. This would be an FCM token for Android, and an APNS token for iOS
    /// - `uaid`: A string representing the users `uaid` that was assigned when the user first registered for a subscription
    /// - `auth`: A string representing an authorization token that will be sent as a header to autopush. The auth was returned on the user's first subscription.
    fn update(&self, new_token: &str, uaid: &str, auth: &str) -> error::Result<()>;

    /// Get a list of server known channels.
    /// # Arguments
    /// - `uaid`: A string representing the users `uaid` that was assigned when the user first registered for a subscription
    /// - `auth`: A string representing an authorization token that will be sent as a header to autopush. The auth was returned on the user's first subscription.
    ///
    /// # Returns
    /// A list of channel ids representing all the channels the user is subscribed to
    fn channel_list(&self, uaid: &str, auth: &str) -> error::Result<Vec<String>>;
}

/// Connect to the Autopush server via the HTTP interface
pub struct ConnectHttp {
    options: PushConfiguration,
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
            pub errno: Option<u32>,
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
            if response.status == status_codes::GONE
                && matches!(response_error.errno, Some(UAID_NOT_FOUND_ERRNO))
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

    fn send_subscription_request<T>(
        &self,
        url: Url,
        headers: Headers,
        registration_id: &str,
        app_server_key: &Option<String>,
    ) -> error::Result<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        let body = RegisterRequest {
            token: registration_id,
            key: app_server_key.as_ref().map(|s| s.as_str()),
        };

        let response = Request::post(url).headers(headers).json(&body).send()?;
        self.check_response_error(&response)?;
        Ok(response.json()?)
    }
}

impl Connection for ConnectHttp {
    fn connect(options: PushConfiguration) -> ConnectHttp {
        ConnectHttp { options }
    }

    fn register(
        &self,
        registration_id: &str,
        app_server_key: &Option<String>,
    ) -> error::Result<RegisterResponse> {
        let url = format!(
            "{}://{}/v1/{}/{}/registration",
            &self.options.http_protocol,
            &self.options.server_host,
            &self.options.bridge_type,
            &self.options.sender_id
        );

        let headers = Headers::new();

        self.send_subscription_request(Url::parse(&url)?, headers, registration_id, app_server_key)
    }

    fn subscribe(
        &self,
        uaid: &str,
        auth: &str,
        registration_id: &str,
        app_server_key: &Option<String>,
    ) -> error::Result<SubscribeResponse> {
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}/subscription",
            &self.options.http_protocol,
            &self.options.server_host,
            &self.options.bridge_type,
            &self.options.sender_id,
            uaid,
        );

        let headers = self.auth_headers(auth)?;

        self.send_subscription_request(Url::parse(&url)?, headers, registration_id, app_server_key)
    }

    fn unsubscribe(&self, channel_id: &str, uaid: &str, auth: &str) -> error::Result<()> {
        let url = format!(
            "{}/subscription/{}",
            self.format_unsubscribe_url(uaid)?,
            channel_id
        );
        let response = Request::delete(Url::parse(&url)?)
            .headers(self.auth_headers(auth)?)
            .send()?;
        info!("unsubscribed from {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
    }

    fn unsubscribe_all(&self, uaid: &str, auth: &str) -> error::Result<()> {
        let url = self.format_unsubscribe_url(uaid)?;
        let response = Request::delete(Url::parse(&url)?)
            .headers(self.auth_headers(auth)?)
            .send()?;
        info!("unsubscribed from all via {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
    }

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
        let body = UpdateRequest { token: new_token };
        let response = Request::put(Url::parse(&url)?)
            .json(&body)
            .headers(self.auth_headers(auth)?)
            .send()?;
        info!("update via {}: {}", url, response.status);
        self.check_response_error(&response)?;
        Ok(())
    }

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
}

#[cfg(test)]
mod test {
    use crate::internal::config::Protocol;

    use super::*;

    use super::Connection;

    use mockito::{mock, server_address};
    use serde_json::json;

    const DUMMY_CHID: &str = "deadbeef00000000decafbad00000000";
    const DUMMY_CHID2: &str = "decafbad00000000deadbeef00000000";

    const DUMMY_UAID: &str = "abad1dea00000000aabbccdd00000000";

    // Local test SENDER_ID ("test*" reserved for Kotlin testing.)
    const SENDER_ID: &str = "FakeSenderID";
    const SECRET: &str = "SuP3rS1kRet";

    #[test]
    fn test_communications() {
        viaduct_reqwest::use_reqwest_backend();
        // mockito forces task serialization, so for now, we test everything in one go.
        let config = PushConfiguration {
            http_protocol: Protocol::Http,
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
            let response = conn.register(SENDER_ID, &None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
        }
        // Second subscription, after first is send with uaid
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
            let response = conn.register(SENDER_ID, &None).unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            assert_eq!(response.channel_id, DUMMY_CHID);
            assert_eq!(response.endpoint, "https://example.com/update");

            let body_2 = json!({
                "uaid": DUMMY_UAID,
                "channelID": DUMMY_CHID2,
                "endpoint": "https://example.com/otherendpoint",
                "senderid": SENDER_ID,
                "secret": SECRET,
            })
            .to_string();
            let ap_mock_2 = mock(
                "POST",
                &*format!(
                    "/v1/fcm/{}/registration/{}/subscription",
                    SENDER_ID, DUMMY_UAID
                ),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_2)
            .create();

            let response = conn
                .subscribe(DUMMY_UAID, SECRET, SENDER_ID, &None)
                .unwrap();
            ap_mock_2.assert();
            assert_eq!(response.endpoint, "https://example.com/otherendpoint");
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
        // we test that we properly return a `AlreadyRegisteredError` when a client
        // gets a `CONFLICT` status code
        {
            let config = PushConfiguration {
                http_protocol: Protocol::Http,
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
            let err = conn.register(SENDER_ID, &None).unwrap_err();
            ap_mock.assert();
            assert!(matches!(err, error::PushError::AlreadyRegisteredError));
        }
    }
}
