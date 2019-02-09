/* Server Communications.
 * Handles however communication to and from the remote Push Server should be done. For Desktop
 * this will be over Websocket. For mobile, it will probably be calls into the local operating
 * system and HTTPS to the web push server.
 *
 * In the future, it could be using gRPC and QUIC, or quantum relay.
 */

extern crate config;
extern crate http;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use std::time::Duration;

use config::PushConfiguration;
use push_errors as error;
use push_errors::ErrorKind::{AlreadyRegisteredError, CommunicationError};
use reqwest::header;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub struct RegisterResponse {
    // the UAID & Channel ID associated with the request
    pub uaid: String,
    pub channelid: String,

    // Auth token for subsequent calls (note, only generated on new UAIDs)
    pub secret: Option<String>,

    // Push endpoint for 3rd parties
    pub endpoint: String,

    // The Sender/Group ID echoed back (if applicable.)
    pub senderid: Option<String>,
}

#[serde(untagged)]
#[derive(Serialize, Deserialize)]
pub enum BroadcastValue {
    Value(String),
    Nested(HashMap<String, BroadcastValue>),
}

pub trait Connection {
    // get the connection UAID
    // TODO [conv]: reset_uaid(). This causes all known subscriptions to be reset.

    // send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &mut self,
        channelid: &str,
        vapid_public_key: Option<&str>,
        registration_token: Option<&str>,
    ) -> error::Result<RegisterResponse>;

    // Drop an endpoint
    fn unsubscribe(&self, channelid: Option<&str>) -> error::Result<bool>;

    // Update an endpoint with new info
    fn update(&self, new_token: &str) -> error::Result<bool>;

    // Get a list of server known channels. If it differs from what we have, reset the UAID, and refresh channels.
    // Should be done once a day.
    fn channel_list(&self) -> error::Result<Vec<String>>;

    // Verify that the known channel list matches up with the server list.
    fn verify_connection(&self, channels: &[String]) -> error::Result<bool>;

    // Regenerate the subscription info for all known, registered channelids
    // Returns HashMap<ChannelID, Endpoint>>
    // In The Future: This should be called by a subscription manager that bundles the returned endpoint along
    // with keys in a Subscription Info object {"endpoint":..., "keys":{"p256dh": ..., "auth": ...}}
    fn regenerate_endpoints(
        &mut self,
        channels: &[String],
        vapid_public_key: Option<&str>,
        registration_token: Option<&str>,
    ) -> error::Result<HashMap<String, String>>;

    // Add one or more new broadcast subscriptions.
    fn broadcast_subscribe(&self, broadcast: BroadcastValue) -> error::Result<BroadcastValue>;

    // get the list of broadcasts
    fn broadcasts(&self) -> error::Result<BroadcastValue>;

    //impl TODO: Handle a Ping response with updated Broadcasts.
    //impl TODO: Handle an incoming Notification
}

pub struct ConnectHttp {
    options: PushConfiguration,
    client: reqwest::Client,
    pub uaid: Option<String>,
    pub auth: Option<String>, // Server auth token
}

// Connect to the Autopush server
pub fn connect(options: PushConfiguration) -> error::Result<ConnectHttp> {
    // find connection via options

    if options.socket_protocol.is_some() && options.http_protocol.is_some() {
        return Err(
            CommunicationError("Both socket and HTTP protocols cannot be set.".to_owned()).into(),
        );
    };
    if options.socket_protocol.is_some() {
        return Err(error::ErrorKind::GeneralError("Unsupported".to_owned()).into());
    };
    let connection = ConnectHttp {
        uaid: None,
        options: options.clone(),
        client: match reqwest::Client::builder()
            .timeout(Duration::from_secs(options.request_timeout))
            .build()
        {
            Ok(v) => v,
            Err(e) => {
                return Err(CommunicationError(format!("Could not build client: {:?}", e)).into());
            }
        },
        auth: None,
    };

    Ok(connection)
}

impl Connection for ConnectHttp {
    /// send a new subscription request to the server, get back the server registration response.
    fn subscribe(
        &mut self,
        channelid: &str,
        vapid_public_key: Option<&str>,
        registration_token: Option<&str>,
    ) -> error::Result<RegisterResponse> {
        // check that things are set
        if self.options.http_protocol.is_none()
            || self.options.bridge_type.is_none()
            || registration_token.is_none()
        {
            return Err(
                CommunicationError("Bridge type or application id not set.".to_owned()).into(),
            );
        }

        let url = format!(
            "{}://{}/v1/{}/{}/registration",
            &self.options.http_protocol.clone().unwrap(),
            &self.options.server_host,
            &self.options.bridge_type.clone().unwrap(),
            &self.options.application_id.clone().unwrap()
        );
        let mut body = HashMap::new();
        body.insert("token", registration_token.unwrap());
        body.insert("channelID", channelid);
        if vapid_public_key.is_some() {
            body.insert("key", vapid_public_key.unwrap());
        }
        let mut request = match self.client.post(&url).json(&body).send() {
            Ok(v) => v,
            Err(e) => {
                return Err(CommunicationError(format!("Could not fetch endpoint: {:?}", e)).into());
            }
        };
        if request.status().is_server_error() {
            dbg!(request);
            return Err(CommunicationError("Server error".to_string()).into());
        }
        if request.status().is_client_error() {
            dbg!(&request);
            if request.status() == http::StatusCode::CONFLICT {
                return Err(AlreadyRegisteredError.into());
            }
            return Err(CommunicationError(format!("Unhandled client error {:?}", request)).into());
        }
        let response: Value = match request.json() {
            Ok(v) => v,
            Err(e) => {
                return Err(CommunicationError(format!("Could not parse response: {:?}", e)).into());
            }
        };

        self.uaid = response["uaid"].as_str().map({ |s| s.to_owned() });
        self.auth = response["secret"].as_str().map({ |s| s.to_owned() });
        Ok(RegisterResponse {
            uaid: self.uaid.clone().unwrap(),
            channelid: response["channelID"].as_str().unwrap().to_owned(),
            secret: self.auth.clone(),
            endpoint: response["endpoint"].as_str().unwrap().to_owned(),
            senderid: response["senderid"].as_str().map({ |s| s.to_owned() }),
        })
    }

    /// Drop a channel and stop recieving updates.
    fn unsubscribe(&self, channel_id: Option<&str>) -> error::Result<bool> {
        if self.auth.is_none() {
            return Err(CommunicationError("Connection is unauthorized".into()).into());
        }
        if self.uaid.is_none() {
            return Err(CommunicationError("No UAID set".into()).into());
        }
        let mut url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &self.options.http_protocol.clone().unwrap(),
            &self.options.server_host,
            &self.options.bridge_type.clone().unwrap(),
            &self.options.application_id.clone().unwrap(),
            &self.uaid.clone().unwrap(),
        );
        if channel_id.is_some() {
            url = format!("{}/subscription/{}", url, channel_id.unwrap())
        }
        match self
            .client
            .delete(&url)
            .header(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&self.auth.clone().unwrap()).unwrap(),
            )
            .send()
        {
            Ok(_) => Ok(true),
            Err(e) => Err(CommunicationError(format!("Could not unsubscribe: {:?}", e)).into()),
        }
    }

    /// Update the push server with the new OS push authorization token
    fn update(&self, new_token: &str) -> error::Result<bool> {
        if self.auth.is_none() {
            return Err(CommunicationError("Connection is unauthorized".into()).into());
        }
        if self.uaid.is_none() {
            return Err(CommunicationError("No UAID set".into()).into());
        }
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &self.options.http_protocol.clone().unwrap(),
            &self.options.server_host,
            &self.options.bridge_type.clone().unwrap(),
            &self.options.application_id.clone().unwrap(),
            &self.uaid.clone().unwrap()
        );
        let mut body = HashMap::new();
        body.insert("token", new_token);
        match self
            .client
            .put(&url)
            .json(&body)
            .header(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&self.auth.clone().unwrap()).unwrap(),
            )
            .send()
        {
            Ok(_) => Ok(true),
            Err(e) => Err(CommunicationError(format!("Could not update token: {:?}", e)).into()),
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
        };

        if self.auth.is_none() {
            return Err(CommunicationError("Connection is unauthorized".into()).into());
        }
        if self.uaid.is_none() {
            return Err(CommunicationError("No UAID set".into()).into());
        }
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}/",
            &self.options.http_protocol.clone().unwrap(),
            &self.options.server_host,
            &self.options.bridge_type.clone().unwrap(),
            &self.options.application_id.clone().unwrap(),
            &self.uaid.clone().unwrap(),
        );
        let mut request = match self
            .client
            .get(&url)
            .header(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&self.auth.clone().unwrap()).unwrap(),
            )
            .send()
        {
            Ok(v) => v,
            Err(e) => {
                return Err(
                    CommunicationError(format!("Could not fetch channel list: {:?}", e)).into(),
                );
            }
        };
        if request.status().is_server_error() {
            dbg!(request);
            return Err(CommunicationError("Server error".to_string()).into());
        }
        if request.status().is_client_error() {
            dbg!(&request);
            return Err(CommunicationError(format!("Unhandled client error {:?}", request)).into());
        }
        let payload: Payload = match request.json() {
            Ok(p) => p,
            Err(e) => {
                return Err(CommunicationError(format!(
                    "Could not fetch channel_list: Bad Response {:?}",
                    e
                ))
                .into());
            }
        };
        if payload.uaid != self.uaid.clone().unwrap() {
            return Err(CommunicationError("Invalid Response from server".to_string()).into());
        }
        Ok(payload.channel_ids.clone())
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
    /// resubscribe all channelids, resulting in new endpoints. This will require sending the
    /// new endpoints to the channel recipient functions.
    fn verify_connection(&self, channels: &[String]) -> error::Result<bool> {
        if self.auth.is_none() {
            return Err(CommunicationError("Connection uninitiated".to_owned()).into());
        }
        let remote = match self.channel_list() {
            Ok(v) => v,
            Err(e) => {
                return Err(
                    CommunicationError(format!("Could not fetch channel list: {:?}", e)).into(),
                );
            }
        };
        // verify both lists match. Either side could have lost it's mind.
        Ok(remote == channels.to_vec())
    }

    /// Fetch new endpoints for a list of channels.
    fn regenerate_endpoints(
        &mut self,
        channels: &[String],
        vapid_public_key: Option<&str>,
        registration_token: Option<&str>,
    ) -> error::Result<HashMap<String, String>> {
        if self.uaid.is_none() {
            return Err(CommunicationError("Connection uninitiated".to_owned()).into());
        }
        let mut results: HashMap<String, String> = HashMap::new();
        for channel in channels {
            let info = self.subscribe(&channel, vapid_public_key, registration_token)?;
            results.insert(channel.clone(), info.endpoint);
        }
        Ok(results)
    }
    //impl TODO: Handle a Ping response with updated Broadcasts.
    //impl TODO: Handle an incoming Notification
}

#[cfg(test)]
mod test {
    use super::*;

    use super::Connection;

    use hex;
    use mockito::{mock, server_address};
    use serde_json::json;

    // use crypto::{get_bytes, Key};

    const DUMMY_CHID: &'static str = "deadbeef00000000decafbad00000000";
    const DUMMY_UAID: &'static str = "abad1dea00000000aabbccdd00000000";
    // Local test SENDER_ID
    const SENDER_ID: &'static str = "308358850242";
    const SECRET: &'static str = "SuP3rS1kRet";

    #[test]
    fn test_success() {
        // mockito forces task serialization, so for now, we test everything in one go.
        // SUBSCRIPTION
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
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                application_id: Some(SENDER_ID.to_owned()),
                bridge_type: Some("test".to_owned()),
                ..Default::default()
            };
            let mut conn = connect(config).unwrap();
            let channel_id = String::from(hex::encode(crypto::get_bytes(16).unwrap()));
            let registration_token = "SomeSytemProvidedRegistrationId";
            let response = conn
                .subscribe(&channel_id, None, Some(registration_token))
                .unwrap();
            ap_mock.assert();
            assert_eq!(response.uaid, DUMMY_UAID);
            // make sure we have stored the secret.
            assert_eq!(conn.auth, Some(SECRET.to_owned()));
        }
        // UNSUBSCRIPTION - Single channel
        {
            let ap_mock = mock(
                "DELETE",
                format!(
                    "/v1/test/{}/registration/{}/subscription/{}",
                    SENDER_ID, DUMMY_UAID, DUMMY_CHID
                )
                .as_ref(),
            )
            .match_header("authorization", SECRET)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                application_id: Some(SENDER_ID.to_owned()),
                bridge_type: Some("test".to_owned()),
                ..Default::default()
            };
            let mut conn = connect(config).unwrap();
            conn.uaid = Some(DUMMY_UAID.to_owned());
            conn.auth = Some(SECRET.to_owned());
            let response = conn.unsubscribe(Some(DUMMY_CHID)).unwrap();
            ap_mock.assert();
            assert!(response);
        }
        // UNSUBSCRIPTION - All for UAID
        {
            let ap_mock = mock(
                "DELETE",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .match_header("authorization", SECRET)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                application_id: Some(SENDER_ID.to_owned()),
                bridge_type: Some("test".to_owned()),
                ..Default::default()
            };
            let mut conn = connect(config).unwrap();
            conn.uaid = Some(DUMMY_UAID.to_owned());
            conn.auth = Some(SECRET.to_owned());
            let response = conn.unsubscribe(None).unwrap();
            ap_mock.assert();
            assert!(response);
        }
        // UPDATE
        {
            let ap_mock = mock(
                "PUT",
                format!("/v1/test/{}/registration/{}", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .match_header("authorization", SECRET)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                application_id: Some(SENDER_ID.to_owned()),
                bridge_type: Some("test".to_owned()),
                ..Default::default()
            };
            let mut conn = connect(config).unwrap();
            conn.uaid = Some(DUMMY_UAID.to_owned());
            conn.auth = Some(SECRET.to_owned());
            let response = conn.update("NewTokenValue").unwrap();
            ap_mock.assert();
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
                format!("/v1/test/{}/registration/{}/", SENDER_ID, DUMMY_UAID).as_ref(),
            )
            .match_header("authorization", SECRET)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_cl_success)
            .create();
            let config = PushConfiguration {
                http_protocol: Some("http".to_owned()),
                server_host: server_address().to_string(),
                application_id: Some(SENDER_ID.to_owned()),
                bridge_type: Some("test".to_owned()),
                ..Default::default()
            };
            let mut conn = connect(config).unwrap();
            conn.uaid = Some(DUMMY_UAID.to_owned());
            conn.auth = Some(SECRET.to_owned());
            let response = conn.channel_list().unwrap();
            ap_mock.assert();
            assert!(response == [DUMMY_CHID.to_owned()]);
        }
    }

}
