mod error;

pub use error::Result;

use serde::Deserialize;
use url::Url;
use viaduct::{header_names, Method, Request};

pub struct RelayClient {
    server_url: String,
    auth_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RelayAddress {
    pub mask_type: String,
    pub enabled: bool,
    pub description: String,
    pub generated_for: String,
    pub block_list_emails: bool,
    pub used_on: String,
    pub id: i64,
    pub address: String,
    pub domain: i64,
    pub full_address: String,
    pub created_at: String, // Use String for timestamps for now (or chrono types later)
    pub last_modified_at: String,
    pub last_used_at: String,
    pub num_forwarded: i64,
    pub num_blocked: i64,
    pub num_level_one_trackers_blocked: i64,
    pub num_replied: i64,
    pub num_spam: i64,
}

impl RelayClient {
    pub fn new(server_url: String, auth_token: Option<String>) -> Self {
        Self {
            server_url,
            auth_token,
        }
    }

    fn build_url(&self, path: &str) -> Result<Url> {
        Ok(Url::parse(&format!("{}{}", self.server_url, path))?)
    }

    fn prepare_request(&self, method: Method, url: Url) -> Result<Request> {
        log::trace!("making {} request to: {}", method.as_str(), url);
        let mut request = Request::new(method, url);
        if let Some(ref token) = self.auth_token {
            request = request.header(header_names::AUTHORIZATION, &format!("Bearer {}", token))?;
        }
        Ok(request)
    }

    pub fn fetch_addresses(&self) -> Result<Vec<RelayAddress>> {
        let url = self.build_url("/api/v1/relayaddresses/")?;
        let request = self.prepare_request(Method::Get, url)?;

        let response = request.send()?;
        log::trace!("response text: {}", response.text());

        let addresses: Vec<RelayAddress> = response.json()?;
        Ok(addresses)
    }

    pub fn accept_terms(&self) -> Result<()> {
        let url = self.build_url("/api/v1/terms-accepted-user/")?;
        let request = self.prepare_request(Method::Post, url)?;

        let response = request.send()?;
        log::trace!("response text: {}", response.text());
        response.require_success()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;

    #[test]
    fn test_fetch_addresses() {
        viaduct_reqwest::use_reqwest_backend();

        let addresses_json = r#"
            [
                {
                    "mask_type": "alias",
                    "enabled": true,
                    "description": "Test Address",
                    "generated_for": "example@example.com",
                    "block_list_emails": false,
                    "used_on": "example.com",
                    "id": 1,
                    "address": "test123",
                    "domain": 123,
                    "full_address": "test123@example.com",
                    "created_at": "2021-01-01T00:00:00Z",
                    "last_modified_at": "2021-01-02T00:00:00Z",
                    "last_used_at": "2021-01-03T00:00:00Z",
                    "num_forwarded": 5,
                    "num_blocked": 1,
                    "num_level_one_trackers_blocked": 0,
                    "num_replied": 2,
                    "num_spam": 0
                }
            ]
        "#;

        let _mock = mock("GET", "/api/v1/relayaddresses/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(addresses_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));

        let addresses = client.fetch_addresses().expect("should fetch addresses");

        assert_eq!(addresses.len(), 1);
        let addr = &addresses[0];
        assert_eq!(addr.full_address, "test123@example.com");
        assert_eq!(addr.generated_for, "example@example.com");
        assert_eq!(addr.enabled, true);
    }

    fn test_accept_terms_with_status(status_code: usize) {
        viaduct_reqwest::use_reqwest_backend();

        let _mock = mock("POST", "/api/v1/terms-accepted-user/")
            .with_status(status_code as usize)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));

        client
            .accept_terms()
            .expect("should accept terms successfully");
    }

    #[test]
    fn test_accept_terms_user_created() {
        test_accept_terms_with_status(201);
    }

    #[test]
    fn test_accept_terms_user_exists() {
        test_accept_terms_with_status(202);
    }
}
