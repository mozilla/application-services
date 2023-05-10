/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::config::RemoteSettingsConfig;
use crate::error::{RemoteSettingsError, Result};
use crate::UniffiCustomTypeConverter;
use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};
use url::Url;
use viaduct::{Request, Response};

const HEADER_BACKOFF: &str = "Backoff";
const HEADER_RETRY_AFTER: &str = "Retry-After";

/// A simple HTTP client that can retrieve Remote Settings data using the properties by [ClientConfig].
/// Methods defined on this will fetch data from
/// <base_url>/v1/buckets/<bucket_name>/collections/<collection_name>/
pub struct Client {
    pub(crate) base_url: Url,
    pub(crate) bucket_name: String,
    pub(crate) collection_name: String,
    pub(crate) remote_state: Mutex<RemoteState>,
}

impl Client {
    /// Create a new [Client] with properties matching config.
    pub fn new(config: RemoteSettingsConfig) -> Result<Self> {
        let server_url = config
            .server_url
            .unwrap_or_else(|| String::from("https://firefox.settings.services.mozilla.com"));
        let bucket_name = config.bucket_name.unwrap_or_else(|| String::from("main"));
        let base_url = Url::parse(&server_url)?;

        Ok(Self {
            base_url,
            bucket_name,
            collection_name: config.collection_name,
            remote_state: Default::default(),
        })
    }

    /// Fetches all records for a collection that can be found in the server,
    /// bucket, and collection defined by the [ClientConfig] used to generate
    /// this [Client].
    pub fn get_records(&self) -> Result<RemoteSettingsResponse> {
        let resp = self.get_records_raw()?;
        let records = resp.json::<RecordsResponse>()?.data;
        let last_modified = resp
            .headers
            .get_as("etag")
            .ok_or_else(|| RemoteSettingsError::ResponseError("no etag header".into()))??;
        Ok(RemoteSettingsResponse {
            records,
            last_modified,
        })
    }

    /// Fetches all records for a collection that can be found in the server,
    /// bucket, and collection defined by the [ClientConfig] used to generate
    /// this [Client]. This function will return the raw network [Response].
    pub fn get_records_raw(&self) -> Result<Response> {
        let path = format!(
            "v1/buckets/{}/collections/{}/records",
            &self.bucket_name, &self.collection_name
        );
        self.make_request(path)
    }

    /// Fetches all records that have been published since provided timestamp
    /// for a collection that can be found in the server, bucket, and
    /// collection defined by the [ClientConfig] used to generate this [Client].
    pub fn get_records_since(&self, timestamp: u64) -> Result<RemoteSettingsResponse> {
        let path = format!(
            "v1/buckets/{}/collections/{}/records?_since={}",
            &self.bucket_name, &self.collection_name, timestamp
        );
        let resp = self.make_request(path)?;
        let records = resp.json::<RecordsResponse>()?.data;
        let last_modified = resp
            .headers
            .get_as("etag")
            .ok_or_else(|| RemoteSettingsError::ResponseError("no etag header".into()))??;
        Ok(RemoteSettingsResponse {
            records,
            last_modified,
        })
    }

    /// Downloads an attachment from [attachment_location]. NOTE: there are no
    /// guarantees about a maximum size, so use care when fetching potentially
    /// large attachments.
    pub fn get_attachment(&self, attachment_location: &str) -> Result<Response> {
        let mut current_remote_state = self.remote_state.lock();
        self.ensure_no_backoff(&mut current_remote_state.backoff)?;
        let attachments_base_url = match &current_remote_state.attachments_base_url {
            Some(url) => url.clone(),
            None => {
                let req = Request::get(self.base_url.clone());
                let server_info = req.send()?.json::<ServerInfo>()?;
                match server_info.capabilities.attachments {
                    Some(capability) => Url::parse(&capability.base_url)?,
                    None => Err(RemoteSettingsError::AttachmentsUnsupportedError)?,
                }
            }
        };
        current_remote_state.attachments_base_url = Some(attachments_base_url.clone());
        drop(current_remote_state);

        self.make_request(attachments_base_url.join(attachment_location)?.into())
    }

    fn make_request(&self, path: String) -> Result<Response> {
        let mut current_remote_state = self.remote_state.lock();
        self.ensure_no_backoff(&mut current_remote_state.backoff)?;
        drop(current_remote_state);

        let url = self.base_url.join(&path)?;
        let req = Request::get(url);
        let resp = req.send()?;

        let mut current_remote_state = self.remote_state.lock();
        self.handle_backoff_hint(&resp, &mut current_remote_state.backoff)?;

        if resp.is_success() {
            Ok(resp)
        } else {
            Err(RemoteSettingsError::ResponseError(resp.text().to_string()))
        }
    }

    fn ensure_no_backoff(&self, current_state: &mut BackoffState) -> Result<()> {
        if let BackoffState::Backoff {
            observed_at,
            duration,
        } = *current_state
        {
            let elapsed_time = observed_at.elapsed();
            if elapsed_time >= duration {
                *current_state = BackoffState::Ok;
            } else {
                let remaining = duration - elapsed_time;
                return Err(RemoteSettingsError::BackoffError(remaining.as_secs()));
            }
        }
        Ok(())
    }

    fn handle_backoff_hint(
        &self,
        response: &Response,
        current_state: &mut BackoffState,
    ) -> Result<()> {
        let extract_backoff_header = |header| -> Result<u64> {
            Ok(response
                .headers
                .get_as::<u64, _>(header)
                .transpose()
                .unwrap_or_default() // Ignore number parsing errors.
                .unwrap_or(0))
        };
        // In practice these two headers are mutually exclusive.
        let backoff = extract_backoff_header(HEADER_BACKOFF)?;
        let retry_after = extract_backoff_header(HEADER_RETRY_AFTER)?;
        let max_backoff = backoff.max(retry_after);

        if max_backoff > 0 {
            *current_state = BackoffState::Backoff {
                observed_at: Instant::now(),
                duration: Duration::from_secs(max_backoff),
            };
        }
        Ok(())
    }
}

/// Data structure representing the top-level response from the Remote Settings.
/// [last_modified] will be extracted from the etag header of the response.
pub struct RemoteSettingsResponse {
    pub records: Vec<Record>,
    pub last_modified: u64,
}

#[derive(Deserialize)]
struct RecordsResponse {
    data: Vec<Record>,
}

/// A parsed Remote Settings record. Records can contain arbitrary fields, so clients
/// are required to further extract expected values from the [fields] member.
#[derive(Deserialize, PartialEq)]
pub struct Record {
    pub id: String,
    pub last_modified: u64,
    pub attachment: Option<Attachment>,
    #[serde(flatten)]
    pub fields: RsJsonObject,
}

/// Attachment metadata that can be optionally attached to a [Record]. The [location] should
/// included in calls to [Client::get_attachment].
#[derive(Deserialize, PartialEq)]
pub struct Attachment {
    pub filename: String,
    pub mimetype: String,
    pub location: String,
    pub hash: String,
    pub size: u64,
}

// At time of writing, UniFFI cannot rename iOS bindings and JsonObject conflicted with the declaration in Nimbus.
// This shouldn't really impact Android, since the type is converted into the platform
// JsonObject thanks to the UniFFI binding.
pub type RsJsonObject = serde_json::Map<String, serde_json::Value>;
impl UniffiCustomTypeConverter for RsJsonObject {
    type Builtin = String;
    fn into_custom(val: Self::Builtin) -> uniffi::Result<Self> {
        let json: serde_json::Value = serde_json::from_str(&val)?;

        match json {
            serde_json::Value::Object(obj) => Ok(obj),
            _ => Err(uniffi::deps::anyhow::anyhow!(
                "Unexpected JSON-non-object in the bagging area"
            )),
        }
    }

    fn from_custom(obj: Self) -> Self::Builtin {
        serde_json::Value::Object(obj).to_string()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteState {
    attachments_base_url: Option<Url>,
    backoff: BackoffState,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self {
            attachments_base_url: None,
            backoff: BackoffState::Ok,
        }
    }
}

/// Used in handling backoff responses from the Remote Settings server.
#[derive(Clone, Copy, Debug)]
pub(crate) enum BackoffState {
    Ok,
    Backoff {
        observed_at: Instant,
        duration: Duration,
    },
}

#[derive(Deserialize)]
struct ServerInfo {
    capabilities: Capabilities,
}

#[derive(Deserialize)]
struct Capabilities {
    attachments: Option<AttachmentsCapability>,
}

#[derive(Deserialize)]
struct AttachmentsCapability {
    base_url: String,
}

#[cfg(test)]
mod test {
    use super::*;
    use mockito::mock;
    #[test]
    fn test_defaults() {
        let config = RemoteSettingsConfig {
            server_url: None,
            bucket_name: None,
            collection_name: String::from("the-collection"),
        };
        let client = Client::new(config).unwrap();
        assert_eq!(
            Url::parse("https://firefox.settings.services.mozilla.com").unwrap(),
            client.base_url
        );
        assert_eq!(String::from("main"), client.bucket_name);
    }

    #[test]
    fn test_attachment_can_be_downloaded() {
        viaduct_reqwest::use_reqwest_backend();
        let server_info_m = mock("GET", "/")
            .with_body(attachment_metadata(mockito::server_url()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .create();

        let attachment_location = "123.jpg";
        let attachment_bytes: Vec<u8> = "I'm a JPG, I swear".into();
        let attachment_m = mock(
            "GET",
            format!("/attachments/{}", attachment_location).as_str(),
        )
        .with_body(attachment_bytes.clone())
        .with_status(200)
        .with_header("content-type", "application/json")
        .create();

        let config = RemoteSettingsConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: None,
        };

        let client = Client::new(config).unwrap();
        let resp = client.get_attachment(attachment_location).unwrap();

        server_info_m.expect(1).assert();
        attachment_m.expect(1).assert();
        assert_eq!(resp.body, attachment_bytes);
    }

    #[test]
    fn test_attachment_errors_if_server_not_configured_for_attachments() {
        viaduct_reqwest::use_reqwest_backend();
        let server_info_m = mock("GET", "/")
            .with_body(NO_ATTACHMENTS_METADATA)
            .with_status(200)
            .with_header("content-type", "application/json")
            .create();

        let attachment_location = "123.jpg";
        let attachment_bytes: Vec<u8> = "I'm a JPG, I swear".into();
        let attachment_m = mock(
            "GET",
            format!("/attachments/{}", attachment_location).as_str(),
        )
        .with_body(attachment_bytes)
        .with_status(200)
        .with_header("content-type", "application/json")
        .create();

        let config = RemoteSettingsConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: None,
        };

        let client = Client::new(config).unwrap();
        let resp = client.get_attachment(attachment_location);
        server_info_m.expect(1).assert();
        attachment_m.expect(0).assert();
        assert!(matches!(
            resp,
            Err(RemoteSettingsError::AttachmentsUnsupportedError)
        ))
    }

    #[test]
    fn test_backoff() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock(
            "GET",
            "/v1/buckets/the-bucket/collections/the-collection/records",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("Backoff", "60")
        .with_header("etag", "1000")
        .create();
        let config = RemoteSettingsConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: Some(String::from("the-bucket")),
        };
        let http_client = Client::new(config).unwrap();

        assert!(http_client.get_records().is_ok());
        let second_resp = http_client.get_records();
        assert!(matches!(
            second_resp,
            Err(RemoteSettingsError::BackoffError(_))
        ));
        m.expect(1).assert();
    }

    #[test]
    fn test_500_retry_after() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock(
            "GET",
            "/v1/buckets/the-bucket/collections/the-collection/records",
        )
        .with_body("Boom!")
        .with_status(500)
        .with_header("Retry-After", "60")
        .create();
        let config = RemoteSettingsConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: Some(String::from("the-bucket")),
        };
        let http_client = Client::new(config).unwrap();
        assert!(http_client.get_records().is_err());
        let second_request = http_client.get_records();
        assert!(matches!(
            second_request,
            Err(RemoteSettingsError::BackoffError(_))
        ));
        m.expect(1).assert();
    }

    #[test]
    fn test_backoff_recovery() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock(
            "GET",
            "/v1/buckets/the-bucket/collections/the-collection/records",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "1000")
        .create();
        let config = RemoteSettingsConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: Some(String::from("the-bucket")),
        };
        let http_client = Client::new(config).unwrap();
        // First, sanity check that manipulating the remote state does something.
        let mut current_remote_state = http_client.remote_state.lock();
        current_remote_state.backoff = BackoffState::Backoff {
            observed_at: Instant::now(),
            duration: Duration::from_secs(30),
        };
        drop(current_remote_state);
        assert!(matches!(
            http_client.get_records(),
            Err(RemoteSettingsError::BackoffError(_))
        ));
        // Then do the actual test.
        let mut current_remote_state = http_client.remote_state.lock();
        current_remote_state.backoff = BackoffState::Backoff {
            observed_at: Instant::now() - Duration::from_secs(31),
            duration: Duration::from_secs(30),
        };
        drop(current_remote_state);
        assert!(http_client.get_records().is_ok());
        m.expect(1).assert();
    }

    fn attachment_metadata(base_url: String) -> String {
        format!(
            r#"
            {{
                "capabilities": {{
                    "admin": {{
                        "description": "Serves the admin console.",
                        "url": "https://github.com/Kinto/kinto-admin/",
                        "version": "2.0.0"
                    }},
                    "attachments": {{
                        "description": "Add file attachments to records",
                        "url": "https://github.com/Kinto/kinto-attachment/",
                        "version": "6.3.1",
                        "base_url": "{}/attachments/"
                    }}
                }}
            }}
    "#,
            base_url
        )
    }

    const NO_ATTACHMENTS_METADATA: &str = r#"
    {
      "capabilities": {
          "admin": {
            "description": "Serves the admin console.",
            "url": "https://github.com/Kinto/kinto-admin/",
            "version": "2.0.0"
          }
      }
    }
  "#;

    fn response_body() -> String {
        format!(
            r#"
        {{
            "data": [
                {},
                {},
                {}
            ]
          }}"#,
            JPG_ATTACHMENT, PDF_ATTACHMENT, NO_ATTACHMENT
        )
    }

    const JPG_ATTACHMENT: &str = r#"
    {
      "title": "jpg-attachment",
      "content": "content",
      "attachment": {
      "filename": "jgp-attachment.jpg",
      "location": "the-bucket/the-collection/d3a5eccc-f0ca-42c3-b0bb-c0d4408c21c9.jpg",
      "hash": "2cbd593f3fd5f1585f92265433a6696a863bc98726f03e7222135ff0d8e83543",
      "mimetype": "image/jpeg",
      "size": 1374325
      },
      "id": "c5dcd1da-7126-4abb-846b-ec85b0d4d0d7",
      "schema": 1677694447771,
      "last_modified": 1677694949407
    }
  "#;

    const PDF_ATTACHMENT: &str = r#"
    {
      "title": "with-attachment",
      "content": "content",
      "attachment": {
          "filename": "pdf-attachment.pdf",
          "location": "the-bucket/the-collection/5f7347c2-af92-411d-a65b-f794f9b5084c.pdf",
          "hash": "de1cde3571ef3faa77ea0493276de9231acaa6f6651602e93aa1036f51181e9b",
          "mimetype": "application/pdf",
          "size": 157
      },
      "id": "ff301910-6bf5-4cfe-bc4c-5c80308661a5",
      "schema": 1677694447771,
      "last_modified": 1677694470354
    }
  "#;

    const NO_ATTACHMENT: &str = r#"
      {
        "title": "no-attachment",
        "content": "content",
        "schema": 1677694447771,
        "id": "7403c6f9-79be-4e0c-a37a-8f2b5bd7ad58",
        "last_modified": 1677694455368
      }
    "#;
}
