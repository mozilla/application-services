/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use serde::Deserialize;
use std::hash::{Hash, Hasher};
use url::Url;
use viaduct::{Headers, Request};

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

impl TryFrom<PreflightResponse> for Headers {
    type Error = viaduct::ViaductError;

    /// Fallible: `geo_location` and `normalized_ua` are echoed straight out of
    /// the MARS response body, and `Headers::insert` rejects a value that is
    /// not printable ASCII. A malformed response must surface as an error, not
    /// as a panic in the caller's process.
    fn try_from(preflight: PreflightResponse) -> Result<Self, Self::Error> {
        let mut headers = Headers::new();
        headers.insert("X-Geo-Location", preflight.geo_location)?;
        if !preflight.normalized_ua.is_empty() {
            headers.insert("X-User-Agent", preflight.normalized_ua)?;
        }
        Ok(headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_carry_geo_location_and_normalized_ua() {
        let headers = Headers::try_from(PreflightResponse {
            geo_location: "US-CA".to_string(),
            normalized_ua: "Firefox/140.0".to_string(),
        })
        .unwrap();

        assert_eq!(headers.get("X-Geo-Location"), Some("US-CA"));
        assert_eq!(headers.get("X-User-Agent"), Some("Firefox/140.0"));
    }

    #[test]
    fn empty_normalized_ua_is_omitted() {
        let headers = Headers::try_from(PreflightResponse {
            geo_location: "US-CA".to_string(),
            normalized_ua: String::new(),
        })
        .unwrap();

        assert_eq!(headers.get("X-Geo-Location"), Some("US-CA"));
        assert_eq!(headers.get("X-User-Agent"), None);
    }

    #[test]
    fn non_ascii_geo_location_is_an_error_not_a_panic() {
        let result = Headers::try_from(PreflightResponse {
            geo_location: "Zürich".to_string(),
            normalized_ua: String::new(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn header_injection_in_normalized_ua_is_an_error_not_a_panic() {
        let result = Headers::try_from(PreflightResponse {
            geo_location: "US-CA".to_string(),
            normalized_ua: "Firefox/140.0\r\nX-Injected: yes".to_string(),
        });

        assert!(result.is_err());
    }
}
