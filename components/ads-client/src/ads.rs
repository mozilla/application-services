/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

/// The ads held for one placement. A placement only ever serves one kind of ad.
#[cfg(feature = "stateful")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Ads {
    Images(Vec<AdImage>),
    Spocs(Vec<AdSpoc>),
    Tiles(Vec<AdTile>),
}

/// Identification of placement sent and returned from MARS (eg: `mock_spoc_1`)
#[cfg(feature = "stateful")]
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct PlacementId(String);

#[cfg(feature = "stateful")]
impl PlacementId {
    pub fn new(s: &str) -> PlacementId {
        PlacementId(s.to_string())
    }
    pub fn into_inner(self) -> String {
        self.0
    }
}

#[cfg(feature = "stateful")]
impl AsRef<str> for PlacementId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(feature = "stateful")]
impl From<String> for PlacementId {
    fn from(value: String) -> Self {
        PlacementId(value)
    }
}

#[cfg(feature = "stateful")]
impl From<PlacementId> for String {
    fn from(value: PlacementId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdImage {
    pub alt_text: Option<String>,
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub format: String,
    pub image_url: Url,
    pub url: Url,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdSpoc {
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub caps: SpocFrequencyCaps,
    pub domain: String,
    pub excerpt: String,
    pub format: String,
    pub image_url: Url,
    pub ranking: SpocRanking,
    pub sponsor: String,
    pub sponsored_by_override: Option<String>,
    pub title: String,
    pub url: Url,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdTile {
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub format: String,
    pub image_url: Url,
    pub name: String,
    pub url: Url,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpocFrequencyCaps {
    pub cap_key: String,
    pub day: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpocRanking {
    pub priority: u32,
    pub personalization_models: Option<HashMap<String, u32>>,
    pub item_score: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}
