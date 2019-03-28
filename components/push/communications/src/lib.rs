/* Server Communications.
 * Handles however communication to and from the remote Push Server should be done. For Desktop
 * this will be over Websocket. For mobile, it will probably be calls into the local operating
 * system and HTTPS to the web push server.
 *
 * In the future, it could be using gRPC and QUIC, or quantum relay.
 */

#![allow(unknown_lints)]

extern crate config;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use serde_json::Value;
use std::collections::HashMap;
use url::Url;
use viaduct::{header_names, status_codes, Request};

use config::PushConfiguration;
use push_errors as error;
use push_errors::ErrorKind::{
    AlreadyRegisteredError, CommunicationError, CommunicationServerError,
};

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

/// A new communication link to the Autopush server
///
pub trait Connection {
    // get the connection UAID
    // TODO [conv]: reset_uaid(). This causes all known subscriptions to be reset.

    // send a new subscription request to the server, get back the server registration response.
    fn subscribe(&mut self, channelid: &str) -> error::Result<RegisterResponse>;

    // Drop an endpoint
    fn unsubscribe(&self, channelid: Option<&str>) -> error::Result<bool>;

    // Update the autopush server with the new native OS Messaging authorization token
    fn update(&mut self, new_token: &str) -> error::Result<bool>;

    // Get a list of server known channels.
    fn channel_list(&self) -> error::Result<Vec<String>>;

    // Verify that the known channel list matches up with the server list. If this fails, regenerate endpoints.
    // This should be performed once a day.
    fn verify_connection(&self, channels: &[String]) -> error::Result<bool>;

    // Add one or more new broadcast subscriptions.
    fn broadcast_subscribe(&self, broadcast: BroadcastValue) -> error::Result<BroadcastValue>;

    // get the list of broadcasts
    fn broadcasts(&self) -> error::Result<BroadcastValue>;

    //impl TODO: Handle a Ping response with updated Broadcasts.
    //impl TODO: Handle an incoming Notification
}

/// Connect to the Autopush server via the HTTP interface
pub struct ConnectHttp {
    pub options: PushConfiguration,
    // pub database: Store,
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
        return Err(error::ErrorKind::CommunicationError("Unsupported".to_owned()).into());
    };
    if options.bridge_type.is_some() && options.registration_id.is_none() {
        return Err(error::ErrorKind::CommunicationError(
            "Missing Registration ID, please register with OS first".to_owned(),
        )
        .into());
    };
    /*    let database = match options.database_path.clone() {
            None => Store::open_in_memory()?,
            Some(path) => Store::open(path)?,
        };
    */
    let connection = ConnectHttp {
        uaid: None,
        options: options.clone(),
        //        database,
        auth: None,
    };

    Ok(connection)
}

impl Connection for ConnectHttp {
    /// send a new subscription request to the server, get back the server registration response.
    fn subscribe(&mut self, channelid: &str) -> error::Result<RegisterResponse> {
        // check that things are set
        if self.options.http_protocol.is_none() || self.options.bridge_type.is_none() {
            return Err(
                CommunicationError("Bridge type or application id not set.".to_owned()).into(),
            );
        }
        let options = self.options.clone();
        let bridge_type = &options.bridge_type.unwrap();
        let url = format!(
            "{}://{}/v1/{}/{}/registration",
            &options.http_protocol.unwrap(),
            &options.server_host,
            &bridge_type,
            &options.sender_id
        );
        let mut body = HashMap::new();
        body.insert("token", options.registration_id.unwrap());
        body.insert("channelID", channelid.to_owned());
        if self.options.vapid_key.is_some() {
            body.insert("key", options.vapid_key.unwrap());
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
                channelid: "deadbeef00000000decafbad00000000".to_owned(),
                secret: self.auth.clone(),
                endpoint: "http://push.example.com/test/opaque".to_owned(),
                senderid: Some(self.options.sender_id.clone()),
            });
        }
        let url = Url::parse(&url)?;
        let requested = match Request::post(url).json(&body).send() {
            Ok(v) => v,
            Err(e) => {
                return Err(
                    CommunicationServerError(format!("Could not fetch endpoint: {:?}", e)).into(),
                );
            }
        };
        if requested.is_server_error() {
            // dbg!(requested);
            return Err(CommunicationServerError("General Server error".to_string()).into());
        }
        if requested.is_client_error() {
            // dbg!(&requested);
            if requested.status == status_codes::CONFLICT {
                return Err(AlreadyRegisteredError.into());
            }
            return Err(CommunicationError(format!("Unhandled client error {:?}", requested)).into());
        }
        let response: Value = match requested.json() {
            Ok(v) => v,
            Err(e) => {
                return Err(
                    CommunicationServerError(format!("Could not parse response: {:?}", e)).into(),
                );
            }
        };

        self.uaid = response["uaid"].as_str().map({ |s| s.to_owned() });
        self.auth = response["secret"].as_str().map({ |s| s.to_owned() });

        let channel_id = response["channelID"].as_str().map({ |s| s.to_owned() });
        let endpoint = response["endpoint"].as_str().map({ |s| s.to_owned() });

        Ok(RegisterResponse {
            uaid: self.uaid.clone().unwrap(),
            channelid: channel_id.unwrap(),
            secret: self.auth.clone(),
            endpoint: endpoint.unwrap(),
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
        let options = self.options.clone();
        let mut url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &options.http_protocol.unwrap(),
            &options.server_host,
            &options.bridge_type.unwrap(),
            &options.sender_id,
            &self.uaid.clone().unwrap(),
        );
        if channel_id.is_some() {
            url = format!("{}/subscription/{}", url, channel_id.unwrap())
        }
        if &self.options.sender_id == "test" {
            return Ok(true);
        }
        match Request::delete(Url::parse(&url)?)
            .header(header_names::AUTHORIZATION, self.auth.clone().unwrap())
            .and_then(|r| r.send())
        {
            Ok(_) => Ok(true),
            Err(e) => {
                Err(CommunicationServerError(format!("Could not unsubscribe: {:?}", e)).into())
            }
        }
    }

    /// Update the push server with the new OS push authorization token
    fn update(&mut self, new_token: &str) -> error::Result<bool> {
        if self.options.sender_id == "test" {
            self.uaid = Some("abad1d3a00000000aabbccdd00000000".to_owned());
            self.auth = Some("LsuUOBKVQRY6-l7_Ajo-Ag".to_owned());
            return Ok(true);
        }
        if self.auth.is_none() {
            return Err(CommunicationError("Connection is unauthorized".into()).into());
        }
        if self.uaid.is_none() {
            return Err(CommunicationError("No UAID set".into()).into());
        }
        self.options.registration_id = Some(new_token.to_owned());
        let options = self.options.clone();
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}",
            &options.http_protocol.unwrap(),
            &options.server_host,
            &options.bridge_type.unwrap(),
            &options.sender_id,
            &self.uaid.clone().unwrap()
        );
        let mut body = HashMap::new();
        body.insert("token", new_token);
        match Request::put(Url::parse(&url)?)
            .json(&body)
            .header(header_names::AUTHORIZATION, self.auth.clone().unwrap())
            .and_then(|r| r.send())
        {
            Ok(_) => Ok(true),
            Err(e) => {
                Err(CommunicationServerError(format!("Could not update token: {:?}", e)).into())
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
        };

        if self.auth.is_none() {
            return Err(CommunicationError("Connection is unauthorized".into()).into());
        }
        if self.uaid.is_none() {
            return Err(CommunicationError("No UAID set".into()).into());
        }
        let options = self.options.clone();
        let url = format!(
            "{}://{}/v1/{}/{}/registration/{}/",
            &options.http_protocol.unwrap(),
            &options.server_host,
            &options.bridge_type.unwrap(),
            &options.sender_id,
            &self.uaid.clone().unwrap(),
        );
        let request = match Request::get(Url::parse(&url)?)
            .header(header_names::AUTHORIZATION, self.auth.clone().unwrap())
            .and_then(|r| r.send())
        {
            Ok(v) => v,
            Err(e) => {
                return Err(CommunicationServerError(format!(
                    "Could not fetch channel list: {:?}",
                    e
                ))
                .into());
            }
        };
        if request.is_server_error() {
            // dbg!(request);
            return Err(CommunicationServerError("Server error".to_string()).into());
        }
        if request.is_client_error() {
            // dbg!(&request);
            return Err(CommunicationError(format!("Unhandled client error {:?}", request)).into());
        }
        let payload: Payload = match request.json() {
            Ok(p) => p,
            Err(e) => {
                return Err(CommunicationServerError(format!(
                    "Could not fetch channel_list: Bad Response {:?}",
                    e
                ))
                .into());
            }
        };
        if payload.uaid != self.uaid.clone().unwrap() {
            return Err(CommunicationServerError("Invalid Response from server".to_string()).into());
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
        if &self.options.sender_id == "test" {
            return Ok(false);
        }
        let remote = match self.channel_list() {
            Ok(v) => v,
            Err(e) => {
                return Err(
                    CommunicationError(format!("Could not fetch channel list: {:?}", e)).into(),
                );
            }
        };
        //let channels = self.database.get_channel_list(&self.uaid.clone().unwrap())?;
        // verify both lists match. Either side could have lost it's mind.
        Ok(remote == channels.to_vec())
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

    const DUMMY_CHID: &str = "deadbeef00000000decafbad00000000";
    const DUMMY_UAID: &str = "abad1dea00000000aabbccdd00000000";
    // Local test SENDER_ID ("test*" reserved for Kotlin testing.)
    const SENDER_ID: &str = "FakeSenderID";
    const SECRET: &str = "SuP3rS1kRet";

    #[test]
    fn test_communications() {
        // mockito forces task serialization, so for now, we test everything in one go.
        let config = PushConfiguration {
            http_protocol: Some("http".to_owned()),
            server_host: server_address().to_string(),
            sender_id: SENDER_ID.to_owned(),
            bridge_type: Some("test".to_owned()),
            registration_id: Some("SomeRegistrationValue".to_owned()),
            ..Default::default()
        };
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
            let mut conn = connect(config.clone()).unwrap();
            let channel_id = hex::encode(crypto::get_bytes(16).unwrap());
            let response = conn.subscribe(&channel_id).unwrap();
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
            let mut conn = connect(config.clone()).unwrap();
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
            let mut conn = connect(config.clone()).unwrap();
            conn.uaid = Some(DUMMY_UAID.to_owned());
            conn.auth = Some(SECRET.to_owned());
            //TODO: Add record to nuke.
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
            let mut conn = connect(config.clone()).unwrap();
            conn.uaid = Some(DUMMY_UAID.to_owned());
            conn.auth = Some(SECRET.to_owned());
            let response = conn.update("NewTokenValue").unwrap();
            ap_mock.assert();
            assert!(conn.options.registration_id == Some("NewTokenValue".to_owned()));
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
            let mut conn = connect(config.clone()).unwrap();
            conn.uaid = Some(DUMMY_UAID.to_owned());
            conn.auth = Some(SECRET.to_owned());
            let response = conn.channel_list().unwrap();
            ap_mock.assert();
            assert!(response == [DUMMY_CHID.to_owned()]);
        }
    }

}
