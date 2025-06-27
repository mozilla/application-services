/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct AdPlacement {
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
    pub placements: Vec<AdPlacement>,
}

#[derive(Debug, Deserialize, uniffi::Record)]
pub struct AdCallbacks {
    pub click: Option<String>,
    pub impression: Option<String>,
    pub report: Option<String>,
}

#[derive(Debug, Deserialize, uniffi::Record)]
pub struct MozAd {
    pub alt_text: Option<String>,
    pub block_key: Option<String>,
    pub callbacks: Option<AdCallbacks>,
    pub format: Option<String>,
    pub image_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, uniffi::Record)]
pub struct AdResponse {
    #[serde(flatten)]
    data: HashMap<String, Vec<MozAd>>,
}
