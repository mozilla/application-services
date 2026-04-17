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

impl From<PreflightResponse> for Headers {
    fn from(preflight: PreflightResponse) -> Self {
        let mut headers = Headers::new();
        headers
            .insert("X-Geo-Location", preflight.geo_location)
            .expect("valid header");
        if !preflight.normalized_ua.is_empty() {
            headers
                .insert("X-User-Agent", preflight.normalized_ua)
                .expect("valid header");
        }
        headers
    }
}
