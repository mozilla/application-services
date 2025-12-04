/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod error;
mod rs;

uniffi::setup_scaffolding!("relay");

pub use error::{ApiResult, Error, RelayApiError, Result};
use error_support::handle_error;

use serde::{Deserialize, Serialize};
use url::Url;
use viaduct::{header_names, Method, Request};

/// Represents a client for the Relay API.
///
/// Use this struct to connect and authenticate with a Relay server,
/// managing authorization to call protected endpoints.
///
/// # Authorization
/// - Clients should use the [fxa_client::FirefoxAccount::getAccessToken()] function
///   to obtain a relay-scoped access token (scope: `https://identity.mozilla.com/apps/relay`).
/// - Then, construct the [`RelayClient`] with the access token.
///   All requests will then be authenticated to the Relay server via `Authorization: Bearer {fxa-access-token}`.
/// - The Relay server verifies this token with the FxA OAuth `/verify` endpoint.
/// - Clients are responsible for getting a new access token when needed.
#[derive(uniffi::Object)]
pub struct RelayClient {
    /// Base URL for the Relay server.
    server_url: String,
    /// Optional authentication token for API requests.
    auth_token: Option<String>,
}

/// Represents a Relay email address object returned by the Relay API.
///
/// Includes metadata and statistics for an alias, such as its status,
/// usage stats, and identifying information.
///
/// See:
/// https://mozilla.github.io/fx-private-relay/api_docs.html
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

/// Represents a bounce status object nested within the profile.
#[derive(Debug, Deserialize, uniffi::Record)]
pub struct BounceStatus {
    pub paused: bool,
    #[serde(rename = "type")]
    pub bounce_type: String,
}

/// Represents a Relay user profile returned by the Relay API.
///
/// Contains information about the user's subscription status, usage statistics,
/// and account settings.
///
/// See: https://mozilla.github.io/fx-private-relay/api_docs.html#tag/privaterelay/operation/profiles_retrieve
#[derive(Debug, Deserialize, uniffi::Record)]
pub struct RelayProfile {
    pub id: i64,
    pub server_storage: bool,
    pub store_phone_log: bool,
    pub subdomain: Option<String>,
    pub has_premium: bool,
    pub has_phone: bool,
    pub has_vpn: bool,
    pub has_megabundle: bool,
    pub onboarding_state: i64,
    pub onboarding_free_state: i64,
    pub date_phone_registered: Option<String>,
    pub date_subscribed: Option<String>,
    pub avatar: Option<String>,
    pub next_email_try: String,
    pub bounce_status: BounceStatus,
    pub api_token: String,
    pub emails_blocked: i64,
    pub emails_forwarded: i64,
    pub emails_replied: i64,
    pub level_one_trackers_blocked: i64,
    pub remove_level_one_email_trackers: Option<bool>,
    pub total_masks: i64,
    pub at_mask_limit: bool,
    pub metrics_enabled: bool,
}

#[derive(Debug, Serialize)]
struct CreateAddressPayload<'a> {
    enabled: bool,
    description: &'a str,
    generated_for: &'a str,
    used_on: &'a str,
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
    /// Creates a new `RelayClient` instance.
    ///
    /// # Parameters
    /// - `server_url`: Base URL for the Relay API.
    /// - `auth_token`: Optional relay-scoped access token (see struct docs).
    ///
    /// # Returns
    /// A new [`RelayClient`] configured for the specified server and token.
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(server_url: String, auth_token: Option<String>) -> ApiResult<Self> {
        Ok(Self {
            server_url,
            auth_token,
        })
    }

    /// Retrieves all Relay addresses associated with the current account.
    ///
    /// Returns a vector of [`RelayAddress`] objects on success.
    ///
    /// ## Errors
    ///
    /// - `RelayApi`: Returned for any non-successful (non-2xx) HTTP response. Provides the HTTP `status` and response `body`; downstream consumers can inspect these fields. If the response body is JSON with `error_code` or `detail` fields, these are parsed and included for more granular handling; otherwise, the raw response text is used as the error detail.
    /// - `Network`: Returned for transport-level failures, like loss of connectivity, with details in `reason`.
    /// - Other variants may be returned for unexpected deserialization, URL, or backend errors.
    #[handle_error(Error)]
    pub fn fetch_addresses(&self) -> ApiResult<Vec<RelayAddress>> {
        let url = self.build_url("/api/v1/relayaddresses/")?;
        let request = self.prepare_request(Method::Get, url)?;

        let response = request.send()?;
        let status = response.status;
        let body = response.text();
        log::trace!("response text: {}", body);

        if status >= 400 {
            return Err(Error::RelayApi {
                status,
                body: body.to_string(),
            });
        }

        let addresses: Vec<RelayAddress> = response.json()?;
        Ok(addresses)
    }

    /// Creates a Relay user record in the Relay service database.
    ///
    /// This function was originally used to signal acceptance of terms and privacy notices,
    /// but now primarily serves to provision (create) the Relay user record if one does not exist.
    ///
    /// ## Errors
    ///
    /// - `RelayApi`: Returned for any non-successful (non-2xx) HTTP response. Provides the HTTP `status` and response `body`; downstream consumers can inspect these fields. If the response body is JSON with `error_code` or `detail` fields, these are parsed and included for more granular handling; otherwise, the raw response text is used as the error detail.
    /// - `Network`: Returned for transport-level failures, like loss of connectivity, with details in `reason`.
    /// - Other variants may be returned for unexpected deserialization, URL, or backend errors.
    #[handle_error(Error)]
    pub fn accept_terms(&self) -> ApiResult<()> {
        let url = self.build_url("/api/v1/terms-accepted-user/")?;
        let request = self.prepare_request(Method::Post, url)?;

        let response = request.send()?;
        let status = response.status;
        let body = response.text();
        log::trace!("response text: {}", body);

        if status >= 400 {
            return Err(Error::RelayApi {
                status,
                body: body.to_string(),
            });
        }
        Ok(())
    }

    /// Creates a new Relay mask (alias) with the specified metadata.
    ///
    /// This is used to generate a new alias for use in an email field.
    ///
    /// - `description`: A label shown in the Relay dashboard; defaults to `generated_for`, user-editable later.
    /// - `generated_for`: The website for which the address is generated.
    /// - `used_on`: Comma-separated list of all websites where this address is used. Only updated by some clients.
    ///
    /// ## Errors
    ///
    /// - `RelayApi`: Returned for any non-successful (non-2xx) HTTP response. Provides the HTTP `status` and response `body`; downstream consumers can inspect these fields. If the response body is JSON with `error_code` or `detail` fields, these are parsed and included for more granular handling; otherwise, the raw response text is used as the error detail.
    /// - `Network`: Returned for transport-level failures, like loss of connectivity, with details in `reason`.
    /// - Other variants may be returned for unexpected deserialization, URL, or backend errors.
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
        let status = response.status;
        let body = response.text();
        log::trace!("response text: {}", body);

        if status >= 400 {
            return Err(Error::RelayApi {
                status,
                body: body.to_string(),
            });
        }

        let address: RelayAddress = response.json()?;
        Ok(address)
    }

    /// Retrieves the profile for the authenticated user.
    ///
    /// Returns a [`RelayProfile`] object containing subscription status, usage statistics,
    /// and account settings. The `has_premium` field indicates whether the user has
    /// an active premium subscription.
    ///
    /// ## Errors
    ///
    /// - `RelayApi`: Returned for any non-successful (non-2xx) HTTP response.
    ///     Provides the HTTP `status` and response `body`; downstream consumers can inspect
    ///     these fields. If the response body is JSON with `error_code` or `detail` fields,
    ///     these are parsed and included for more granular handling; otherwise, the raw
    ///     response text is used as the error detail.
    /// - `Network`: Returned for transport-level failures, like loss of connectivity,
    ///     with details in `reason`.
    /// - Other variants may be returned for unexpected deserialization, URL, or backend errors.
    #[handle_error(Error)]
    pub fn fetch_profile(&self) -> ApiResult<RelayProfile> {
        let url = self.build_url("/api/v1/profiles/")?;
        let request = self.prepare_request(Method::Get, url)?;

        let response = request.send()?;
        let status = response.status;
        let body = response.text();
        log::trace!("response text: {}", body);

        if status >= 400 {
            return Err(Error::RelayApi {
                status,
                body: body.to_string(),
            });
        }

        // The API returns an array with a single profile object for the authenticated user
        let profiles: Vec<RelayProfile> = response.json()?;
        profiles.into_iter().next().ok_or_else(|| Error::RelayApi {
            status: 200,
            body: "No profile found for authenticated user".to_string(),
        })
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
    fn test_fetch_addresses_permission_denied_relay_account() {
        viaduct_dev::init_backend_dev();

        let error_json = r#"{"detail": "Authenticated user does not have a Relay account. Have they accepted the terms?"}"#;
        let _mock = mock("GET", "/api/v1/relayaddresses/")
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(error_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));
        let result = client.expect("success").fetch_addresses();

        match result {
            Err(RelayApiError::Api {
                status,
                code,
                detail,
            }) => {
                assert_eq!(status, 403);
                assert_eq!(code, "unknown"); // No error_code present in JSON
                assert_eq!(
                    detail,
                    "Authenticated user does not have a Relay account. Have they accepted the terms?"
                );
            }
            other => panic!("Expected RelayApiError::Api but got {:?}", other),
        }
    }

    #[test]
    fn test_accept_terms_parse_error_missing_token() {
        viaduct_dev::init_backend_dev();

        let error_json = r#"{"detail": "Missing FXA Token after 'Bearer'."}"#;
        let _mock = mock("POST", "/api/v1/terms-accepted-user/")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(error_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), None);
        let result = client.expect("success").accept_terms();

        match result {
            Err(RelayApiError::Api {
                status,
                code,
                detail,
            }) => {
                assert_eq!(status, 400);
                assert_eq!(code, "unknown"); // No error_code present in JSON
                assert_eq!(detail, "Missing FXA Token after 'Bearer'.");
            }
            other => panic!("Expected RelayApiError::Api but got {:?}", other),
        }
    }

    #[test]
    fn test_create_address_free_tier_limit() {
        viaduct_dev::init_backend_dev();

        let error_json = r#"{"error_code": "free_tier_limit", "detail": "You’ve used all 5 email masks included with your free account."}"#;
        let _mock = mock("POST", "/api/v1/relayaddresses/")
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(error_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));
        let result = client
            .expect("success")
            .create_address("Label", "example.com", "example.com");

        match result {
            Err(RelayApiError::Api {
                status,
                code,
                detail,
            }) => {
                assert_eq!(status, 403);
                assert_eq!(code, "free_tier_limit");
                assert_eq!(
                    detail,
                    "You’ve used all 5 email masks included with your free account."
                );
            }
            other => panic!("Expected RelayApiError::Api but got {:?}", other),
        }
    }

    #[test]
    fn test_fetch_addresses() {
        viaduct_dev::init_backend_dev();

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
        viaduct_dev::init_backend_dev();

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
        viaduct_dev::init_backend_dev();

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
        viaduct_dev::init_backend_dev();

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
        viaduct_dev::init_backend_dev();

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

    fn mock_profile_json(
        id: i64,
        has_premium: bool,
        subdomain: Option<&str>,
        total_masks: i64,
        at_mask_limit: bool,
        emails_forwarded: i64,
        emails_blocked: i64,
    ) -> String {
        let subdomain_json = subdomain
            .map(|s| format!(r#""{}""#, s))
            .unwrap_or_else(|| "null".to_string());
        let date_subscribed = if has_premium {
            r#""2023-01-10T08:00:00Z""#
        } else {
            "null"
        };
        let date_phone_registered = if has_premium {
            r#""2023-01-15T10:30:00Z""#
        } else {
            "null"
        };
        let avatar = if has_premium {
            r#""https://example.com/avatar.png""#
        } else {
            "null"
        };
        let remove_level_one_email_trackers = if has_premium { "true" } else { "null" };

        format!(
            r#"
        [
            {{
                "id": {id},
                "server_storage": {has_premium},
                "store_phone_log": {has_premium},
                "subdomain": {subdomain_json},
                "has_premium": {has_premium},
                "has_phone": {has_premium},
                "has_vpn": false,
                "has_megabundle": false,
                "onboarding_state": 5,
                "onboarding_free_state": 0,
                "date_phone_registered": {date_phone_registered},
                "date_subscribed": {date_subscribed},
                "avatar": {avatar},
                "next_email_try": "2023-12-01T00:00:00Z",
                "bounce_status": {{
                    "paused": false,
                    "type": "none"
                }},
                "api_token": "550e8400-e29b-41d4-a716-446655440000",
                "emails_blocked": {emails_blocked},
                "emails_forwarded": {emails_forwarded},
                "emails_replied": 10,
                "level_one_trackers_blocked": 42,
                "remove_level_one_email_trackers": {remove_level_one_email_trackers},
                "total_masks": {total_masks},
                "at_mask_limit": {at_mask_limit},
                "metrics_enabled": true
            }}
        ]
        "#
        )
    }

    #[test]
    fn test_fetch_profile_premium_user() {
        viaduct_dev::init_backend_dev();

        let profile_json = mock_profile_json(123, true, Some("testuser"), 15, false, 150, 25);

        let _mock = mock("GET", "/api/v1/profiles/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(profile_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));

        let profile = client
            .expect("success")
            .fetch_profile()
            .expect("should fetch profile");

        assert_eq!(profile.id, 123);
        assert!(profile.has_premium);
        assert_eq!(profile.total_masks, 15);
        assert!(!profile.at_mask_limit);
        assert_eq!(profile.subdomain, Some("testuser".to_string()));
        assert!(profile.has_phone);
        assert!(!profile.has_vpn);
        assert_eq!(profile.emails_forwarded, 150);
        assert_eq!(profile.emails_blocked, 25);
    }

    #[test]
    fn test_fetch_profile_free_user() {
        viaduct_dev::init_backend_dev();

        let profile_json = mock_profile_json(456, false, None, 5, true, 20, 5);

        let _mock = mock("GET", "/api/v1/profiles/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(profile_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("mock_token".to_string()));

        let profile = client
            .expect("success")
            .fetch_profile()
            .expect("should fetch profile");

        assert_eq!(profile.id, 456);
        assert!(!profile.has_premium);
        assert_eq!(profile.total_masks, 5);
        assert!(profile.at_mask_limit);
        assert_eq!(profile.subdomain, None);
        assert!(!profile.has_phone);
        assert_eq!(profile.date_subscribed, None);
    }

    #[test]
    fn test_fetch_profile_unauthorized() {
        viaduct_dev::init_backend_dev();

        let error_json = r#"{"detail": "Authentication credentials were not provided."}"#;
        let _mock = mock("GET", "/api/v1/profiles/")
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(error_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), None);
        let result = client.expect("success").fetch_profile();

        match result {
            Err(RelayApiError::Api {
                status,
                code,
                detail,
            }) => {
                assert_eq!(status, 403);
                assert_eq!(code, "unknown");
                assert_eq!(detail, "Authentication credentials were not provided.");
            }
            other => panic!("Expected RelayApiError::Api but got {:?}", other),
        }
    }

    #[test]
    fn test_fetch_profile_invalid_token() {
        viaduct_dev::init_backend_dev();

        let error_json = r#"{"error_code": "invalid_token", "detail": "Invalid FXA token."}"#;
        let _mock = mock("GET", "/api/v1/profiles/")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(error_json)
            .create();

        let client = RelayClient::new(mockito::server_url(), Some("bad_token".to_string()));
        let result = client.expect("success").fetch_profile();

        match result {
            Err(RelayApiError::Api {
                status,
                code,
                detail,
            }) => {
                assert_eq!(status, 401);
                assert_eq!(code, "invalid_token");
                assert_eq!(detail, "Invalid FXA token.");
            }
            other => panic!("Expected RelayApiError::Api but got {:?}", other),
        }
    }
}
