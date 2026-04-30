/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::error::{Error, Result};
use super::models::response::{WcsLiveMatchesResponse, WcsMatchesResponse};
use url::Url;
use viaduct::{header_names, Request, Response};

pub struct HttpClient;

pub trait HttpClientTrait {
    fn make_live_matches_request(
        &self,
        teams: Option<&str>,
        endpoint_url: Url,
    ) -> Result<WcsLiveMatchesResponse>;

    fn make_matches_request(
        &self,
        date: Option<&str>,
        limit: Option<i32>,
        teams: Option<&str>,
        endpoint_url: Url,
    ) -> Result<WcsMatchesResponse>;
}

fn build_live_url(teams: Option<&str>, mut endpoint_url: Url) -> Url {
    if let Some(teams) = teams {
        endpoint_url.query_pairs_mut().append_pair("teams", teams);
    }
    endpoint_url
}

fn build_matches_url(
    date: Option<&str>,
    limit: Option<i32>,
    teams: Option<&str>,
    mut endpoint_url: Url,
) -> Url {
    if date.is_none() && limit.is_none() && teams.is_none() {
        return endpoint_url;
    }
    {
        let mut params = endpoint_url.query_pairs_mut();
        if let Some(date) = date {
            params.append_pair("date", date);
        }
        if let Some(limit) = limit {
            params.append_pair("limit", &limit.to_string());
        }
        if let Some(teams) = teams {
            params.append_pair("teams", teams);
        }
    }
    endpoint_url
}

fn send_get(url: Url) -> Result<Response> {
    let response: Response = Request::get(url)
        .header(header_names::ACCEPT, "application/json")?
        .send()?;
    Ok(response)
}

fn check_status(response: &Response) -> Result<()> {
    let status = response.status;
    if status >= 400 {
        let message = response.text().to_string();
        return Err(match status {
            400 => Error::BadRequest { code: status, message },
            422 => Error::Validation { code: status, message },
            500..=599 => Error::Server { code: status, message },
            _ => Error::Unexpected { code: status, message },
        });
    }
    Ok(())
}

impl HttpClientTrait for HttpClient {
    fn make_live_matches_request(
        &self,
        teams: Option<&str>,
        endpoint_url: Url,
    ) -> Result<WcsLiveMatchesResponse> {
        let url = build_live_url(teams, endpoint_url);
        let response = send_get(url)?;
        check_status(&response)?;
        Ok(response.json()?)
    }

    fn make_matches_request(
        &self,
        date: Option<&str>,
        limit: Option<i32>,
        teams: Option<&str>,
        endpoint_url: Url,
    ) -> Result<WcsMatchesResponse> {
        let url = build_matches_url(date, limit, teams, endpoint_url);
        let response = send_get(url)?;
        check_status(&response)?;
        Ok(response.json()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE_URL: &str = "https://merino.services.mozilla.com/api/v1/wcs/live";
    const MATCHES_URL: &str = "https://merino.services.mozilla.com/api/v1/wcs/matches";

    fn live_url() -> Url {
        Url::parse(LIVE_URL).unwrap()
    }

    fn matches_url() -> Url {
        Url::parse(MATCHES_URL).unwrap()
    }

    fn has_param(url: &Url, key: &str, value: &str) -> bool {
        url.query_pairs().any(|(k, v)| k == key && v == value)
    }

    #[test]
    fn test_build_live_url_no_filter() {
        let url = build_live_url(None, live_url());
        assert_eq!(url.as_str(), LIVE_URL);
    }

    #[test]
    fn test_build_live_url_with_teams() {
        let url = build_live_url(Some("BRA,ARG"), live_url());
        assert!(has_param(&url, "teams", "BRA,ARG"));
    }

    #[test]
    fn test_build_matches_url_no_params() {
        let url = build_matches_url(None, None, None, matches_url());
        assert_eq!(url.as_str(), MATCHES_URL);
    }

    #[test]
    fn test_build_matches_url_with_all_params() {
        let url = build_matches_url(Some("2026-06-15"), Some(5), Some("BRA,ARG"), matches_url());
        assert!(has_param(&url, "date", "2026-06-15"));
        assert!(has_param(&url, "limit", "5"));
        assert!(has_param(&url, "teams", "BRA,ARG"));
    }

    #[test]
    fn test_build_matches_url_with_date_only() {
        let url = build_matches_url(Some("2026-06-15"), None, None, matches_url());
        assert!(has_param(&url, "date", "2026-06-15"));
        let keys: Vec<_> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
        assert!(!keys.contains(&"limit".to_string()));
        assert!(!keys.contains(&"teams".to_string()));
    }
}
