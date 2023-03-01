/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This is a simple HTTP client that uses viaduct to retrieve Remote Settings data from the server.

use std::time::{Duration, Instant};

use crate::config::ClientConfig;
use crate::error::{ClientError, Result};
use std::cell::Cell;
use url::Url;
use viaduct::{status_codes, Request, Response};

const HEADER_BACKOFF: &str = "Backoff";
const HEADER_RETRY_AFTER: &str = "Retry-After";

/// A simple HTTP client that can retrieve Remote Settings data using the properties by [ClientConfig].
/// Methods defined on this will fetch data from
/// <base_url>/v1/buckets/<bucket_name>/collections/<collection_name>/
pub struct Client {
    pub(crate) base_url: Url,
    pub(crate) bucket_name: String,
    pub(crate) collection_name: String,
    pub(crate) remote_state: Cell<RemoteState>,
}

/// Used in handling backoff responses from the Remote Settings server.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RemoteState {
    Ok,
    Backoff {
        observed_at: Instant,
        duration: Duration,
    },
}

impl Client {
    /// Create a new [Client] with properties matching config.
    pub fn new(config: ClientConfig) -> Result<Self> {
        let server_url = config
            .server_url
            .unwrap_or_else(|| String::from("https://firefox.settings.services.mozilla.com"));
        let bucket_name = config.bucket_name.unwrap_or_else(|| String::from("main"));

        Ok(Self {
            base_url: Url::parse(&server_url)?,
            bucket_name,
            collection_name: config.collection_name,
            remote_state: Cell::new(RemoteState::Ok),
        })
    }

    /// Fetches all records for a collection that can be found in the server,
    /// bucket, and collection defined by the [ClientConfig] used to generate
    /// this [Client].
    pub fn get_records(&self) -> Result<Response> {
        let path = format!(
            "v1/buckets/{}/collections/{}/records",
            &self.bucket_name, &self.collection_name
        );
        let url = self.base_url.join(&path)?;
        let req = Request::get(url);
        self.make_request(req)
    }

    fn make_request(&self, request: Request) -> Result<Response> {
        self.ensure_no_backoff()?;
        let resp = request.send()?;
        self.handle_backoff_hint(&resp)?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            Err(ClientError::ResponseError(resp.text().to_string()))
        }
    }

    fn ensure_no_backoff(&self) -> Result<()> {
        if let RemoteState::Backoff {
            observed_at,
            duration,
        } = self.remote_state.get()
        {
            let elapsed_time = observed_at.elapsed();
            if elapsed_time >= duration {
                self.remote_state.replace(RemoteState::Ok);
            } else {
                let remaining = duration - elapsed_time;
                return Err(ClientError::BackoffError(remaining.as_secs()));
            }
        }
        Ok(())
    }

    fn handle_backoff_hint(&self, response: &Response) -> Result<()> {
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
            self.remote_state.replace(RemoteState::Backoff {
                observed_at: Instant::now(),
                duration: Duration::from_secs(max_backoff),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use mockito::mock;
    #[test]
    fn test_defaults() {
        let config = ClientConfig {
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
        .create();
        let config = ClientConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: Some(String::from("the-bucket")),
        };
        let http_client = Client::new(config).unwrap();

        // let url = Url::parse(&format!("{}/{}", &base_url, path)).unwrap();
        assert!(http_client.get_records().is_ok());
        let second_resp = http_client.get_records();
        assert!(matches!(second_resp, Err(ClientError::BackoffError(_))));
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
        let config = ClientConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: Some(String::from("the-bucket")),
        };
        let http_client = Client::new(config).unwrap();
        assert!(http_client.get_records().is_err());
        let second_request = http_client.get_records();
        assert!(matches!(second_request, Err(ClientError::BackoffError(_))));
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
        .create();
        let config = ClientConfig {
            server_url: Some(mockito::server_url()),
            collection_name: String::from("the-collection"),
            bucket_name: Some(String::from("the-bucket")),
        };
        let mut http_client = Client::new(config).unwrap();
        // First, sanity check that manipulating the remote state does something.
        http_client.remote_state.replace(RemoteState::Backoff {
            observed_at: Instant::now(),
            duration: Duration::from_secs(30),
        });
        assert!(matches!(
            http_client.get_records(),
            Err(ClientError::BackoffError(_))
        ));
        // Then do the actual test.
        http_client.remote_state = Cell::new(RemoteState::Backoff {
            observed_at: Instant::now() - Duration::from_secs(31),
            duration: Duration::from_secs(30),
        });
        assert!(http_client.get_records().is_ok());
        m.expect(1).assert();
    }

    fn response_body() -> String {
        r#"
        {{ "data": [
            {{
                "empty_field": null,
                "bool_field": true,
                "int_field": 123,
                "string_field": "value"
            }}
        ]}}"#
            .to_string()
    }
}
