/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_derive::*;

use crate::error::{MerinoClientError, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerinoClientOptions {
    pub endpoint_url: String,
    pub client_variants: Vec<String>,
    pub timeout_ms: i64,
    pub providers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerinoClientFetchOptions {
    pub providers: Vec<String>,
    pub timeout_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MerinoSuggestion {
    pub title: String,
    pub url: String,
    pub provider: String,
    pub is_sponsored: bool,
    pub score: f64,
    pub icon: Option<String>,
    pub request_id: String,
    pub client_variants: Vec<String>,
}

pub struct MerinoClient {
    // ...
}

impl MerinoClient {
    pub fn new(options: MerinoClientOptions) -> Self {
        Self {
            // ...
        }
    }

    pub fn fetch(&self, query: &str, options: Option<MerinoClientFetchOptions>) -> Result<Vec<MerinoSuggestion>> {
        Err(MerinoClientError::Other { reason: "Not implemented".into() })
    }
}
