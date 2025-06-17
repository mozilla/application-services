/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod error;

uniffi::setup_scaffolding!("relay");

pub use error::{ApiResult, Error, RelayApiError, Result};
use error_support::handle_error;

use serde::{Deserialize, Serialize};
use url::Url;
use viaduct::{header_names, Method, Request};

#[derive(uniffi::Object)]
pub struct RelayClient {
    server_url: String,
    auth_token: Option<String>,
}

#[derive(Debug, Deserialize, uniffi::Record)]
pub struct RelayAddress {
    pub mask_type: String,
    pub enabled: bool,
    pub description: String,
    pub generated_for: String,
    pub block_list_emails: bool,
    pub used_on: Option<String>,
    pub id: i64,
    pub address: String,
    pub domain: i64,
    pub full_address: String,
    pub created_at: String, // Use String for timestamps for now (or chrono types later)
    pub last_modified_at: String,
    pub last_used_at: Option<String>,
    pub num_forwarded: i64,
    pub num_blocked: i64,
    pub num_level_one_trackers_blocked: i64,
    pub num_replied: i64,
    pub num_spam: i64,
}

#[derive(Debug, Serialize)]
struct CreateAddressPayload<'a> {
    enabled: bool,
    description: &'a str,
    generated_for: &'a str,
    used_on: &'a str,
}

#[derive(Deserialize)]
struct RelayApiErrorMessage {
    detail: String,
}

impl RelayClient {
    fn build_url(&self, path: &str) -> Result<Url> {
        Ok(Url::parse(&format!("{}{}", self.server_url, path))?)
    }

    fn prepare_request(&self, method: Method, url: Url) -> Result<Request> {
        log::trace!("making {} request to: {}", method.as_str(), url);
        let mut request = Request::new(method, url);
        if let Some(ref token) = self.auth_token {
            request = request.header(header_names::AUTHORIZATION, format!("Bearer {}", token))?;
        }
        Ok(request)
    }
}

#[uniffi::export]
impl RelayClient {
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(server_url: String, auth_token: Option<String>) -> ApiResult<Self> {
        Ok(Self {
            server_url,
            auth_token,
        })
    }

    #[handle_error(Error)]
    pub fn fetch_addresses(&self) -> ApiResult<Vec<RelayAddress>> {
        let url = self.build_url("/api/v1/relayaddresses/")?;
        let request = self.prepare_request(Method::Get, url)?;

        let response = request.send()?;
        let body = response.text();
        log::trace!("response text: {}", body);
        if let Ok(parsed) = serde_json::from_str::<RelayApiErrorMessage>(&body) {
            return Err(Error::RelayApi(parsed.detail));
        }

        let addresses: Vec<RelayAddress> = response.json()?;
        Ok(addresses)
    }

    #[handle_error(Error)]
    pub fn accept_terms(&self) -> ApiResult<()> {
        let url = self.build_url("/api/v1/terms-accepted-user/")?;
        let request = self.prepare_request(Method::Post, url)?;

        let response = request.send()?;
        let body = response.text();
        log::trace!("response text: {}", body);
        if let Ok(parsed) = serde_json::from_str::<RelayApiErrorMessage>(&body) {
            return Err(Error::RelayApi(parsed.detail));
        }
        Ok(())
    }

    #[handle_error(Error)]
    pub fn create_address(
        &self,
        description: &str,
        generated_for: &str,
        used_on: &str,
    ) -> ApiResult<RelayAddress> {
        let url = self.build_url("/api/v1/relayaddresses/")?;

        let payload = CreateAddressPayload {
            enabled: true,
            description,
            generated_for,
            used_on,
        };

        let mut request = self.prepare_request(Method::Post, url)?;
        request = request.json(&payload);

        let response = request.send()?;
        let body = response.text();
        log::trace!("response text: {}", body);
        if let Ok(parsed) = serde_json::from_str::<RelayApiErrorMessage>(&body) {
            return Err(Error::RelayApi(parsed.detail));
        }

        let address: RelayAddress = response.json()?;
        Ok(address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;

    fn base_addresses_json(extra_fields: &str) -> String {
        format!(
            r#"
            [
                {{
                    "mask_type": "random",
                    "enabled": true,
                    "description": "Base Address",
                    "generated_for": "example.com",
                    "block_list_emails": false,
                    "id": 1,
                    "address": "base12345",
                    "domain": 2,
                    "full_address": "base12345@mozmail.com",
                    "created_at": "2021-01-01T00:00:00Z",
                    "last_modified_at": "2021-01-02T00:00:00Z",
                    {extra_fields}
                    "num_forwarded": 5,
                    "num_blocked": 1,
                    "num_level_one_trackers_blocked": 0,
                    "num_replied": 2,
                    "num_spam": 0
                }}
            ]
            "#
        )
    }

    #[test]
    fn test_fetch_addresses() {
        viaduct_reqwest::use_reqwest_backend();

        let addresses_json = base_addresses_json(
            r#""used_on": "example.com", "last_used_at": "2021-01-03T00:00:00Z", "#,
        );
        log::trace!("addresses_json: {}", addresses_json);

        let _mock = mock("GET", "/api/v1/relayaddresses/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(addresses_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));

        let addresses = client
            .expect("success")
            .fetch_addresses()
            .expect("should fetch addresses");

        assert_eq!(addresses.len(), 1);
        let addr = &addresses[0];
        assert!(addr.enabled);
        assert_eq!(addr.full_address, "base12345@mozmail.com");
        assert_eq!(addr.generated_for, "example.com");
    }

    #[test]
    fn test_fetch_addresses_used_on_null() {
        viaduct_reqwest::use_reqwest_backend();

        let addresses_json =
            base_addresses_json(r#""used_on": null,"last_used_at": "2021-01-03T00:00:00Z","#);

        let _mock = mock("GET", "/api/v1/relayaddresses/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(addresses_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));
        let addresses = client
            .expect("success")
            .fetch_addresses()
            .expect("should fetch addresses");

        assert_eq!(addresses.len(), 1);
        assert_eq!(addresses[0].used_on, None);
    }

    #[test]
    fn test_fetch_addresses_last_used_at_null() {
        viaduct_reqwest::use_reqwest_backend();

        let addresses_json =
            base_addresses_json(r#""used_on": "example.com","last_used_at": null,"#);

        let _mock = mock("GET", "/api/v1/relayaddresses/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(addresses_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));
        let addresses = client
            .expect("success")
            .fetch_addresses()
            .expect("should fetch addresses");

        assert_eq!(addresses.len(), 1);
        assert_eq!(addresses[0].last_used_at, None);
    }

    fn test_accept_terms_response(
        status_code: usize,
        body: Option<&str>,
        token: Option<&str>,
        expect_error: bool,
    ) {
        viaduct_reqwest::use_reqwest_backend();

        let mut mock = mock("POST", "/api/v1/terms-accepted-user/").with_status(status_code);

        if let Some(body_text) = body {
            mock = mock
                .with_header("content-type", "application/json")
                .with_body(body_text);
        }

        let _mock = mock.create();
        let client = RelayClient::new(mockito::server_url(), token.map(String::from));

        let result = client.expect("success").accept_terms();

        if expect_error {
            assert!(result.is_err(), "Expected error but got success.");
        } else {
            assert!(result.is_ok(), "Expected success but got error.");
        }
    }

    #[test]
    fn test_accept_terms_user_created() {
        test_accept_terms_response(201, None, Some("mock_token"), false);
    }

    #[test]
    fn test_accept_terms_user_exists() {
        test_accept_terms_response(202, None, Some("mock_token"), false);
    }

    #[test]
    fn test_accept_terms_missing_authorization_header() {
        test_accept_terms_response(
            400,
            Some(r#"{"detail": "Missing Bearer header."}"#),
            None,
            true,
        );
    }

    #[test]
    fn test_accept_terms_invalid_token() {
        test_accept_terms_response(
            403,
            Some(r#"{"detail": "Invalid token."}"#),
            Some("invalid_token"),
            true,
        );
    }

    #[test]
    fn test_accept_terms_server_error_profile_failure() {
        test_accept_terms_response(
            500,
            Some(r#"{"detail": "Did not receive a 200 response for account profile."}"#),
            Some("valid_token_but_profile_fails"),
            true,
        );
    }

    #[test]
    fn test_accept_terms_user_not_found() {
        test_accept_terms_response(
            404,
            Some(r#"{"detail": "FXA user not found."}"#),
            Some("valid_token_but_user_missing"),
            true,
        );
    }

    #[test]
    fn test_create_address() {
        viaduct_reqwest::use_reqwest_backend();

        let address_json = r#"
        {
            "mask_type": "alias",
            "enabled": true,
            "description": "Created Address",
            "generated_for": "example.com",
            "block_list_emails": false,
            "used_on": "example.com",
            "id": 2,
            "address": "new123456",
            "domain": 2,
            "full_address": "new123456@mozmail.com",
            "created_at": "2021-01-04T00:00:00Z",
            "last_modified_at": "2021-01-05T00:00:00Z",
            "last_used_at": "2021-01-06T00:00:00Z",
            "num_forwarded": 3,
            "num_blocked": 0,
            "num_level_one_trackers_blocked": 0,
            "num_replied": 1,
            "num_spam": 0
        }
    "#;

        let _mock = mock("POST", "/api/v1/relayaddresses/")
            .match_header("authorization", "Bearer mock_token")
            .match_header("content-type", "application/json")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(address_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));

        let address = client
            .expect("success")
            .create_address("Created Address", "example.com", "example.com")
            .expect("should create address successfully");

        assert_eq!(address.full_address, "new123456@mozmail.com");
        assert_eq!(address.generated_for, "example.com");
        assert!(address.enabled);
    }
}
