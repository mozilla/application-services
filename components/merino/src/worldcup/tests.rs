/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::*;
use url::Url;
use viaduct::{Headers, Method, Response};

const TEAMS_RESPONSE: &str = include_str!("fixtures/teams.json");
const MATCH_RESPONSE: &str = include_str!("fixtures/matches.json");
const LIVE_RESPONSE: &str = include_str!("fixtures/live.json");

fn make_response(status: u16, body: &str, url: Url, accept_language: Option<String>) -> Response {
    let mut headers = Headers::new();
    if let Some(lang) = accept_language {
        headers.insert("accept-language", lang).unwrap();
    }
    Response {
        request_method: Method::Get,
        url,
        status,
        headers,
        body: body.as_bytes().to_vec(),
    }
}

fn default_options() -> WorldCupOptions {
    WorldCupOptions {
        limit: None,
        teams: None,
        accept_language: None,
    }
}

fn base_url() -> Url {
    Url::parse(DEFAULT_BASE_URL).unwrap()
}

struct FakeHttpClient(fn(http::WorldCupQueryParams) -> Result<Option<Response>>);

impl http::HttpClientTrait for FakeHttpClient {
    fn make_request(
        &self,
        _url: Url,
        params: http::WorldCupQueryParams,
    ) -> Result<Option<Response>> {
        (self.0)(params)
    }
}

struct FakeCapturingClient {
    captured_url: std::sync::Arc<std::sync::Mutex<Option<Url>>>,
}

impl http::HttpClientTrait for FakeCapturingClient {
    fn make_request(
        &self,
        url: Url,
        params: http::WorldCupQueryParams,
    ) -> Result<Option<Response>> {
        *self.captured_url.lock().unwrap() = Some(http::build_url(url.clone(), &params));
        Ok(Some(make_response(200, "{}", url, None)))
    }
}

#[test]
fn test_get_teams_success() {
    fn response(_params: http::WorldCupQueryParams) -> Result<Option<Response>> {
        Ok(Some(make_response(
            200,
            TEAMS_RESPONSE,
            Url::parse("https://merino.services.mozilla.com/api/v1/wcs/").unwrap(),
            None,
        )))
    }
    let client = WorldCupClientInner::new_with_client(FakeHttpClient(response));
    let result = client.make_request(base_url().join("teams").unwrap(), default_options());
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap().text(), TEAMS_RESPONSE);
}

#[test]
fn test_get_matches_success() {
    fn response(_params: http::WorldCupQueryParams) -> Result<Option<Response>> {
        Ok(Some(make_response(
            200,
            MATCH_RESPONSE,
            Url::parse("https://merino.services.mozilla.com/api/v1/wcs/").unwrap(),
            None,
        )))
    }
    let client = WorldCupClientInner::new_with_client(FakeHttpClient(response));
    let result = client.make_request(base_url().join("matches").unwrap(), default_options());
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap().text(), MATCH_RESPONSE);
}

#[test]
fn test_get_live_success() {
    fn response(_params: http::WorldCupQueryParams) -> Result<Option<Response>> {
        Ok(Some(make_response(
            200,
            LIVE_RESPONSE,
            Url::parse("https://merino.services.mozilla.com/api/v1/wcs/").unwrap(),
            None,
        )))
    }
    let client = WorldCupClientInner::new_with_client(FakeHttpClient(response));
    let result = client.make_request(base_url().join("live").unwrap(), default_options());
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap().text(), LIVE_RESPONSE);
}

#[test]
fn test_no_content_returns_none() {
    fn response(_params: http::WorldCupQueryParams) -> Result<Option<Response>> {
        Ok(None)
    }
    let client = WorldCupClientInner::new_with_client(FakeHttpClient(response));

    let result = client.make_request(base_url().join("teams").unwrap(), default_options());
    assert!(result.unwrap().is_none());

    let result = client.make_request(base_url().join("matches").unwrap(), default_options());
    assert!(result.unwrap().is_none());

    let result = client.make_request(base_url().join("live").unwrap(), default_options());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_server_error() {
    fn response(_params: http::WorldCupQueryParams) -> Result<Option<Response>> {
        Err(Error::Server {
            code: 500,
            message: "Internal server error".to_string(),
        })
    }
    let client = WorldCupClientInner::new_with_client(FakeHttpClient(response));
    let result = client.make_request(base_url().join("teams").unwrap(), default_options());
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        Error::Server { code: 500, .. }
    ));
}

#[test]
fn test_accept_language_is_passed_as_header() {
    fn response(params: http::WorldCupQueryParams) -> Result<Option<Response>> {
        Ok(Some(make_response(
            200,
            "",
            Url::parse("https://merino.services.mozilla.com/api/v1/wcs/").unwrap(),
            params.accept_language,
        )))
    }
    let client = WorldCupClientInner::new_with_client(FakeHttpClient(response));
    let result = client.make_request(
        base_url().join("teams").unwrap(),
        WorldCupOptions {
            limit: None,
            teams: None,
            accept_language: Some("en-US".to_string()),
        },
    );
    let response = result.unwrap().unwrap();
    assert_eq!(response.headers.get("accept-language"), Some("en-US"));
}

#[test]
fn test_builder_uses_default_base_host() {
    let client = WorldCupClientBuilder::new().build().unwrap();
    assert_eq!(client.base_url.as_str(), DEFAULT_BASE_URL);
}

#[test]
fn test_builder_uses_custom_base_host() {
    let client = WorldCupClientBuilder::new()
        .base_host("https://stage.merino.services.mozilla.com".to_string())
        .build()
        .unwrap();
    assert_eq!(
        client.base_url.as_str(),
        "https://stage.merino.services.mozilla.com/api/v1/wcs/"
    );
}

#[test]
fn test_teams_endpoint_url() {
    let captured_url = std::sync::Arc::new(std::sync::Mutex::new(None::<Url>));
    let client_inner = WorldCupClientInner::new_with_client(FakeCapturingClient {
        captured_url: captured_url.clone(),
    });
    let _ = client_inner.make_request(base_url().join("teams").unwrap(), default_options());
    let captured = captured_url.lock().unwrap();
    assert_eq!(
        captured.as_ref().unwrap().as_str(),
        "https://merino.services.mozilla.com/api/v1/wcs/teams"
    );
}

#[test]
fn test_matches_endpoint_url_with_limit() {
    let captured_url = std::sync::Arc::new(std::sync::Mutex::new(None::<Url>));
    let client_inner = WorldCupClientInner::new_with_client(FakeCapturingClient {
        captured_url: captured_url.clone(),
    });
    let _ = client_inner.make_request(
        base_url().join("matches").unwrap(),
        WorldCupOptions {
            limit: Some(2),
            teams: None,
            accept_language: None,
        },
    );
    let captured = captured_url.lock().unwrap();
    assert_eq!(
        captured.as_ref().unwrap().as_str(),
        "https://merino.services.mozilla.com/api/v1/wcs/matches?limit=2"
    );
}

#[test]
fn test_live_endpoint_url() {
    let captured_url = std::sync::Arc::new(std::sync::Mutex::new(None::<Url>));
    let client_inner = WorldCupClientInner::new_with_client(FakeCapturingClient {
        captured_url: captured_url.clone(),
    });
    let _ = client_inner.make_request(base_url().join("live").unwrap(), default_options());
    let captured = captured_url.lock().unwrap();
    assert_eq!(
        captured.as_ref().unwrap().as_str(),
        "https://merino.services.mozilla.com/api/v1/wcs/live"
    );
}

#[test]
fn test_builder_fails_with_invalid_base_host() {
    let result = WorldCupClientBuilder::new()
        .base_host("not a valid url".to_string())
        .build();
    match result {
        Err(Error::UrlParse(_)) => {}
        Err(other) => panic!("Expected UrlParse error, got: {:?}", other),
        Ok(_) => panic!("Expected error for invalid base_host"),
    }
}
