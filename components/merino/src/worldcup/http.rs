/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use url::Url;
use viaduct::{Request, Response};

use super::error::{Error, Result};

pub struct HttpClient;

#[derive(Default)]
pub struct WorldCupQueryParams {
    pub limit: Option<u32>,
    pub teams: Option<String>,
    pub accept_language: Option<String>,
}

pub trait HttpClientTrait {
    fn make_request(&self, url: Url, params: WorldCupQueryParams) -> Result<Option<Response>>;
}

impl HttpClientTrait for HttpClient {
    fn make_request(&self, url: Url, params: WorldCupQueryParams) -> Result<Option<Response>> {
        send_get(build_url(url, &params), params.accept_language)
    }
}

pub fn build_url(endpoint_url: Url, params: &WorldCupQueryParams) -> Url {
    if params.limit.is_none() && params.teams.is_none() {
        return endpoint_url;
    }
    let mut url = endpoint_url;
    {
        let mut pairs = url.query_pairs_mut();
        if let Some(v) = params.limit {
            pairs.append_pair("limit", &v.to_string());
        }
        if let Some(v) = &params.teams {
            pairs.append_pair("teams", v);
        }
    }
    url
}

fn send_get(url: Url, accept_language: Option<String>) -> Result<Option<Response>> {
    let mut request = Request::get(url).header("accept", "application/json")?;
    if let Some(lang) = accept_language {
        request = request.header("accept-language", lang)?;
    }
    let response = request.send()?;
    let status = response.status;
    match status {
        200 => Ok(Some(response)),
        204 => Ok(None),
        400 => Err(Error::BadRequest {
            code: status,
            message: response.text().to_string(),
        }),
        422 => Err(Error::Validation {
            code: status,
            message: response.text().to_string(),
        }),
        500..=599 => Err(Error::Server {
            code: status,
            message: response.text().to_string(),
        }),
        _ => Err(Error::Unexpected {
            code: status,
            message: response.text().to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_URL: &str = "https://merino.services.mozilla.com/api/v1/wcs/teams";

    fn base_url() -> Url {
        Url::parse(BASE_URL).unwrap()
    }

    fn has_param(url: &Url, key: &str, value: &str) -> bool {
        url.query_pairs().any(|(k, v)| k == key && v == value)
    }

    #[test]
    fn test_build_url_with_params() {
        let options = WorldCupQueryParams {
            limit: Some(10),
            ..WorldCupQueryParams::default()
        };
        let url = build_url(base_url(), &options);
        assert!(has_param(&url, "limit", "10"));
    }

    #[test]
    fn test_build_url_with_teams() {
        let options = WorldCupQueryParams {
            teams: Some("FRA,ENG".to_string()),
            ..WorldCupQueryParams::default()
        };
        let url = build_url(base_url(), &options);
        assert!(has_param(&url, "teams", "FRA,ENG"));
    }

    #[test]
    fn test_build_url_with_all_options() {
        let options = WorldCupQueryParams {
            limit: Some(5),
            teams: Some("FRA,ENG".to_string()),
            accept_language: Some("en-GB".to_string()),
        };
        let url = build_url(base_url(), &options);
        assert!(has_param(&url, "limit", "5"));
        assert!(has_param(&url, "teams", "FRA,ENG"));
    }

    #[test]
    fn test_build_url_omits_none_options() {
        let url = build_url(base_url(), &WorldCupQueryParams::default());
        let keys: Vec<_> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
        assert_eq!(keys.len(), 0);
    }

    #[test]
    fn test_build_url_full_string() {
        let options = WorldCupQueryParams {
            limit: Some(3),
            teams: Some("FRA".to_string()),
            accept_language: Some("en-US".to_string()),
        };
        let url = build_url(base_url(), &options);
        assert_eq!(
            url.to_string(),
            "https://merino.services.mozilla.com/api/v1/wcs/teams\
            ?limit=3\
            &teams=FRA"
        );
    }
}
