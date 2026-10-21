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
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AdImageOrSpoc {
    Image(AdImage),
    Spoc(AdSpoc),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AdFormat {
    Rectangle,
    Spoc,
    Tile,
    Billboard,
    Skyscraper,
    Leaderboard,
}

impl AdFormat {
    pub fn is_image(&self) -> bool {
        matches!(
            self,
            AdFormat::Rectangle
                | AdFormat::Billboard
                | AdFormat::Skyscraper
                | AdFormat::Leaderboard
        )
    }

    pub fn is_spoc(&self) -> bool {
        matches!(self, AdFormat::Spoc)
    }
}

impl<'de> Deserialize<'de> for AdImageOrSpoc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let value = <serde_json::Value>::deserialize(deserializer)?;
        let format_str = value
            .get("format")
            .and_then(|v| v.as_str())
            .ok_or(D::Error::custom(
                "`format` field is missing or could not be parsed".to_string(),
            ))?;
        let format: AdFormat =
            serde_json::from_str(&format!("\"{format_str}\"")).map_err(D::Error::custom)?;

        if format.is_image() {
            Ok(AdImageOrSpoc::Image(
                serde_json::from_value(value).map_err(D::Error::custom)?,
            ))
        } else if format.is_spoc() {
            Ok(AdImageOrSpoc::Spoc(
                serde_json::from_value(value).map_err(D::Error::custom)?,
            ))
        } else {
            Err(D::Error::custom(format!(
                "Unrecognized format: {format_str}"
            )))
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpocFrequencyCaps {
    pub cap_key: String,
    pub day: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpocRanking {
    pub item_score: f64,
    pub personalization_models: Option<HashMap<String, u32>>,
    pub priority: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}

#[cfg(test)]
mod tests {
    use crate::{ads::AdImageOrSpoc, test_utils::get_example_happy_spoc_response};

    #[test]
    fn ad_image_or_spoc_deserialization() {
        use crate::test_utils::get_example_happy_image_response;
        let happy_image_example = get_example_happy_image_response()
            .data
            .into_values()
            .next()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let happy_spoc_example = get_example_happy_spoc_response()
            .data
            .into_values()
            .next()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let happy_image_example_str = serde_json::to_string(&happy_image_example).unwrap();
        let happy_spoc_example_str = serde_json::to_string(&happy_spoc_example).unwrap();

        let deserialized_image: AdImageOrSpoc =
            serde_json::from_str(&happy_image_example_str).unwrap();
        assert!(matches!(deserialized_image, AdImageOrSpoc::Image(_)));

        let deserialized_spoc: AdImageOrSpoc =
            serde_json::from_str(&happy_spoc_example_str).unwrap();
        assert!(matches!(deserialized_spoc, AdImageOrSpoc::Spoc(_)));

        // Changing the spocs 'format' field to an image should result in it parsing to a image.
        let mut happy_spoc_renamed_example = happy_spoc_example.clone();
        happy_spoc_renamed_example.format = "billboard".to_string();
        let happy_spoc_renamed_example_str =
            serde_json::to_string(&happy_spoc_renamed_example).unwrap();
        let deserialized_image: AdImageOrSpoc =
            serde_json::from_str(&happy_spoc_renamed_example_str).unwrap();
        assert!(matches!(deserialized_image, AdImageOrSpoc::Image(_)));

        // Changing the spocs 'format' field to something unrecognized should result in it parsing to an error.
        let mut happy_spoc_renamed_example = happy_spoc_example.clone();
        happy_spoc_renamed_example.format = "nonsense_format".to_string();
        let happy_spoc_renamed_example_str =
            serde_json::to_string(&happy_spoc_renamed_example).unwrap();
        let deserialized_spoc: Result<AdImageOrSpoc, _> =
            serde_json::from_str(&happy_spoc_renamed_example_str);
        assert!(deserialized_spoc.is_err());
    }
}
