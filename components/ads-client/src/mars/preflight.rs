/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use serde::Deserialize;
use std::hash::{Hash, Hasher};
use url::Url;
use viaduct::{Headers, Request};

use super::error::CallbackRequestError;

pub struct PreflightRequest(pub Url);

impl Hash for PreflightRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl From<PreflightRequest> for Request {
    fn from(req: PreflightRequest) -> Self {
        Request::get(req.0)
    }
}

/// Response from the MARS `/v1/ads-preflight` endpoint.
#[derive(Debug, Deserialize)]
pub struct PreflightResponse {
    #[serde(default)]
    pub geo_location: String,
    #[serde(default)]
    pub normalized_ua: String,
}

impl PreflightResponse {
    /// The headers MARS reads from an OHTTP request.
    ///
    /// MARS needs `X-User-Agent` on every OHTTP request. It is the preflight's
    /// `normalized_ua`, or `fallback_user_agent` when the preflight sent none.
    pub fn into_headers(
        self,
        fallback_user_agent: Option<&str>,
    ) -> Result<Headers, CallbackRequestError> {
        let user_agent = if self.normalized_ua.is_empty() {
            fallback_user_agent
                .filter(|ua| !ua.is_empty())
                .ok_or(CallbackRequestError::MissingUserAgent)?
        } else {
            &self.normalized_ua
        };
        let mut headers = Headers::new();
        headers.insert("X-Geo-Location", self.geo_location.as_str())?;
        headers.insert("X-User-Agent", user_agent)?;
        Ok(headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preflight(geo_location: &str, normalized_ua: &str) -> PreflightResponse {
        PreflightResponse {
            geo_location: geo_location.to_string(),
            normalized_ua: normalized_ua.to_string(),
        }
    }

    #[test]
    fn headers_carry_geo_location_and_normalized_ua() {
        let headers = preflight("US-CA", "Firefox/140.0")
            .into_headers(None)
            .unwrap();

        assert_eq!(headers.get("X-Geo-Location"), Some("US-CA"));
        assert_eq!(headers.get("X-User-Agent"), Some("Firefox/140.0"));
    }

    #[test]
    fn normalized_ua_wins_over_the_fallback() {
        let headers = preflight("US-CA", "Firefox/140.0")
            .into_headers(Some(
                "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0",
            ))
            .unwrap();

        assert_eq!(headers.get("X-User-Agent"), Some("Firefox/140.0"));
    }

    #[test]
    fn empty_normalized_ua_falls_back_to_the_browser_user_agent() {
        let browser_ua = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";
        let headers = preflight("US-CA", "")
            .into_headers(Some(browser_ua))
            .unwrap();

        assert_eq!(headers.get("X-Geo-Location"), Some("US-CA"));
        assert_eq!(headers.get("X-User-Agent"), Some(browser_ua));
    }

    #[test]
    fn no_user_agent_at_all_is_an_error() {
        let result = preflight("US-CA", "").into_headers(None);
        assert!(matches!(
            result,
            Err(CallbackRequestError::MissingUserAgent)
        ));

        let result = preflight("US-CA", "").into_headers(Some(""));
        assert!(matches!(
            result,
            Err(CallbackRequestError::MissingUserAgent)
        ));
    }

    #[test]
    fn non_ascii_geo_location_is_an_error_not_a_panic() {
        let result = preflight("Zürich", "Firefox/140.0").into_headers(None);

        assert!(result.is_err());
    }

    #[test]
    fn header_injection_in_normalized_ua_is_an_error_not_a_panic() {
        let result = preflight("US-CA", "Firefox/140.0\r\nX-Injected: yes").into_headers(None);

        assert!(result.is_err());
    }

    #[test]
    fn header_injection_in_fallback_user_agent_is_an_error_not_a_panic() {
        let result = preflight("US-CA", "").into_headers(Some("Firefox/140.0\r\nX-Injected: yes"));

        assert!(result.is_err());
    }
}
