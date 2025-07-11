/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(match opt {
        Some(s) if s.trim().is_empty() => None,
        other => other,
    })
}

#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct AdPlacementRequest {
    pub placement: String,
    pub count: u32,
    pub content: Option<AdContentCategory>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct AdContentCategory {
    pub taxonomy: String,
    pub categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct AdRequest {
    pub context_id: String,
    pub placements: Vec<AdPlacementRequest>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct AdCallbacks {
    #[serde(deserialize_with = "empty_string_as_none")]
    pub click: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub impression: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub report: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct MozAd {
    pub alt_text: Option<String>,
    pub block_key: Option<String>,
    pub callbacks: Option<AdCallbacks>,
    pub format: Option<String>,
    pub image_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct AdResponse {
    #[serde(flatten)]
    pub data: HashMap<String, Vec<MozAd>>,
}
