/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use serde::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

use crate::error::RequestAdsError;

#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct AdResponse {
    #[serde(deserialize_with = "AdResponse::deserialize_ad_response", flatten)]
    pub data: HashMap<String, Vec<Ad>>,
}

impl AdResponse {
    fn deserialize_ad_response<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<String, Vec<Ad>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = HashMap::<String, serde_json::Value>::deserialize(deserializer)?;
        let mut result = HashMap::new();

        for (key, value) in raw {
            if let serde_json::Value::Array(arr) = value {
                let mut ads: Vec<Ad> = vec![];
                for item in arr {
                    if let Ok(ad) = serde_json::from_value::<Ad>(item) {
                        ads.push(ad);
                    } else {
                        #[cfg(not(test))]
                        {
                            use crate::instrument::{emit_telemetry_event, TelemetryEvent};
                            let _ = emit_telemetry_event(Some(TelemetryEvent::InvalidUrlError));
                        }
                    }
                }
                if !ads.is_empty() {
                    result.insert(key, ads);
                }
            }
        }

        Ok(result)
    }

    pub fn filter<T>(&self) -> Result<HashMap<String, Vec<T>>, RequestAdsError>
    where
        T: FromAd,
    {
        let mut result = HashMap::new();
        for (placement_id, ads) in &self.data {
            let mut found_items = Vec::new();
            let mut found_other_type = None;

            for ad in ads {
                if let Some(item) = T::from_ad(ad) {
                    found_items.push(item);
                } else {
                    found_other_type = Some(match ad {
                        Ad::Image(_) => std::any::type_name::<AdImage>().to_string(),
                        Ad::Spoc(_) => std::any::type_name::<AdSpoc>().to_string(),
                        Ad::UATile(_) => std::any::type_name::<AdUATile>().to_string(),
                    });
                    break;
                }
            }

            if let Some(other_type) = found_other_type {
                return Err(RequestAdsError::UnexpectedAdType {
                    placement_id: placement_id.clone(),
                    expected_type: std::any::type_name::<T>().to_string(),
                    found_type: other_type,
                });
            }

            if !found_items.is_empty() {
                result.insert(placement_id.clone(), found_items);
            }
        }
        Ok(result)
    }

    pub fn filter_and_take_first<T>(&self) -> Result<HashMap<String, T>, RequestAdsError>
    where
        T: FromAd,
    {
        let filtered = self.filter::<T>()?;
        Ok(filtered
            .into_iter()
            .filter_map(|(k, mut v)| {
                if v.is_empty() {
                    None
                } else {
                    Some((k, v.remove(0)))
                }
            })
            .collect())
    }
}

pub trait FromAd: Clone {
    fn from_ad(ad: &Ad) -> Option<Self>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Ad {
    Spoc(AdSpoc),
    UATile(AdUATile),
    // IMPORTANT: Image must be last because it has fewer fields than Spoc/UATile.
    // With untagged enums, serde tries variants in order, so we need to try the more
    // specific types first to avoid incorrectly deserializing Spoc/UATile ads as Image.
    Image(AdImage),
}

impl FromAd for AdImage {
    fn from_ad(ad: &Ad) -> Option<Self> {
        match ad {
            Ad::Image(img) => Some(img.clone()),
            _ => None,
        }
    }
}

impl FromAd for AdSpoc {
    fn from_ad(ad: &Ad) -> Option<Self> {
        match ad {
            Ad::Spoc(spoc) => Some(spoc.clone()),
            _ => None,
        }
    }
}

impl FromAd for AdUATile {
    fn from_ad(ad: &Ad) -> Option<Self> {
        match ad {
            Ad::UATile(tile) => Some(tile.clone()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdImage {
    pub alt_text: Option<String>,
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub format: String,
    pub image_url: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdSpoc {
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub caps: SpocFrequencyCaps,
    pub domain: String,
    pub excerpt: String,
    pub format: String,
    pub image_url: String,
    pub ranking: SpocRanking,
    pub sponsor: String,
    pub sponsored_by_override: Option<String>,
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdUATile {
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub format: String,
    pub image_url: String,
    pub name: String,
    pub url: String,
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

#[cfg(test)]
mod tests {
    use crate::test_utils::get_example_happy_image_response;

    use super::*;
    use serde_json::{from_str, json};

    #[test]
    fn test_moz_ad_full() {
        let response_full = json!({
            "alt_text": "An ad for an anvil",
            "block_key": "abc123",
            "callbacks": {
                "click": "https://buyanvilseveryday.test/click",
                "impression": "https://buyanvilseveryday.test/impression",
                "report": "https://buyanvilseveryday.test/report"
            },
            "format": "Leaderboard",
            "image_url": "https://buyanvilseveryday.test/img.png",
            "url": "https://buyanvilseveryday.test"
        })
        .to_string();

        let full: Ad = from_str(&response_full).unwrap();
        assert_eq!(
            full,
            Ad::Image(AdImage {
                alt_text: Some("An ad for an anvil".into()),
                block_key: "abc123".into(),
                callbacks: AdCallbacks {
                    click: Url::parse("https://buyanvilseveryday.test/click").unwrap(),
                    impression: Url::parse("https://buyanvilseveryday.test/impression").unwrap(),
                    report: Some(Url::parse("https://buyanvilseveryday.test/report").unwrap()),
                },
                format: "Leaderboard".into(),
                image_url: "https://buyanvilseveryday.test/img.png".into(),
                url: "https://buyanvilseveryday.test".into(),
            })
        );
    }

    #[test]
    fn test_moz_ad_response_partial() {
        let response_partial = json!({
            "alt_text": null,
            "block_key": "abc123",
            "callbacks": {
                "click": "https://example.test/click",
                "impression": "https://example.test/impression",
                "report": null
            },
            "format": "Leaderboard",
            "image_url": "https://example.test/image.png",
            "url": "https://example.test/item"
        })
        .to_string();

        let partial: Ad = from_str(&response_partial).unwrap();
        assert_eq!(
            partial,
            Ad::Image(AdImage {
                alt_text: None,
                block_key: "abc123".into(),
                callbacks: AdCallbacks {
                    click: Url::parse("https://example.test/click").unwrap(),
                    impression: Url::parse("https://example.test/impression").unwrap(),
                    report: None,
                },
                format: "Leaderboard".into(),
                image_url: "https://example.test/image.png".into(),
                url: "https://example.test/item".into(),
            })
        );
    }

    #[test]
    fn test_ad_response_serialization() {
        let raw_ad_response = json!({
            "missing_click_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_1",
                    "image_url": "https://ads.fakeexample.org/example_image_1",
                    "format": "billboard",
                    "alt_text": "An ad for a puppy",
                    "callbacks": {
                        "impression": "https://ads.fakeexample.org/impression/example_ad_1",
                    }
                }
            ],
            "incorrect_click_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_1",
                    "image_url": "https://ads.fakeexample.org/example_image_1",
                    "format": "billboard",
                    "alt_text": "An ad for a puppy",
                    "callbacks": {
                        "click": "incorrect-click-url",
                        "impression": "https://ads.fakeexample.org/impression/example_ad_1",
                    }
                }
            ],
            "missing_impression_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_1",
                    "image_url": "https://ads.fakeexample.org/example_image_1",
                    "format": "billboard",
                    "alt_text": "An ad for a puppy",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_1",
                    }
                }
            ],
            "incorrect_impression_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_2",
                    "image_url": "https://ads.fakeexample.org/example_image_2",
                    "format": "skyscraper",
                    "alt_text": "An ad for a pet duck",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_2",
                        "impression": "incorrect-impression-url",
                    }
                }
            ],
            "valid_ad": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_3",
                    "image_url": "https://ads.fakeexample.org/example_image_3",
                    "format": "skyscraper",
                    "alt_text": "An ad for a pet duck",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_3",
                        "impression": "https://ads.fakeexample.org/impression/example_ad_3",
                        "report": "https://ads.fakeexample.org/report/example_ad_3"
                    }
                }
            ]
        })
        .to_string();

        let parsed: AdResponse = from_str(&raw_ad_response).unwrap();

        let expected = AdResponse {
            data: HashMap::from([(
                "valid_ad".to_string(),
                vec![Ad::Image(AdImage {
                    url: "https://ads.fakeexample.org/example_ad_3".to_string(),
                    image_url: "https://ads.fakeexample.org/example_image_3".to_string(),
                    format: "skyscraper".to_string(),
                    block_key: "abc123".into(),
                    alt_text: Some("An ad for a pet duck".to_string()),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://ads.fakeexample.org/click/example_ad_3")
                            .unwrap(),
                        impression: Url::parse(
                            "https://ads.fakeexample.org/impression/example_ad_3",
                        )
                        .unwrap(),
                        report: Some(
                            Url::parse("https://ads.fakeexample.org/report/example_ad_3").unwrap(),
                        ),
                    },
                })],
            )]),
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_empty_ad_response_serialization() {
        let raw_ad_response = json!({
            "example_placement_1": [],
            "example_placement_2": []
        })
        .to_string();

        let parsed: AdResponse = from_str(&raw_ad_response).unwrap();

        let expected = AdResponse {
            data: HashMap::from([]),
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_filter_image_ads() {
        let response = get_example_happy_image_response();
        let result = response.filter::<AdImage>();

        assert!(result.is_ok());
        let filtered = result.unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains_key("example_placement_1"));
        assert!(filtered.contains_key("example_placement_2"));
        assert_eq!(filtered.get("example_placement_1").unwrap().len(), 1);
        assert_eq!(filtered.get("example_placement_2").unwrap().len(), 1);
    }

    #[test]
    fn test_filter_spoc_ads() {
        let mut response = AdResponse {
            data: HashMap::new(),
        };
        response.data.insert(
            "placement_1".to_string(),
            vec![
                Ad::Spoc(AdSpoc {
                    block_key: "key1".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://example.com/click").unwrap(),
                        impression: Url::parse("https://example.com/impression").unwrap(),
                        report: None,
                    },
                    caps: SpocFrequencyCaps {
                        cap_key: "cap1".to_string(),
                        day: 1,
                    },
                    domain: "example.com".to_string(),
                    excerpt: "Test excerpt".to_string(),
                    format: "spoc".to_string(),
                    image_url: "https://example.com/image.png".to_string(),
                    ranking: SpocRanking {
                        priority: 1,
                        personalization_models: None,
                        item_score: 0.5,
                    },
                    sponsor: "Sponsor".to_string(),
                    sponsored_by_override: None,
                    title: "Test Title".to_string(),
                    url: "https://example.com".to_string(),
                }),
                Ad::Spoc(AdSpoc {
                    block_key: "key2".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://example.com/click2").unwrap(),
                        impression: Url::parse("https://example.com/impression2").unwrap(),
                        report: None,
                    },
                    caps: SpocFrequencyCaps {
                        cap_key: "cap2".to_string(),
                        day: 2,
                    },
                    domain: "example2.com".to_string(),
                    excerpt: "Test excerpt 2".to_string(),
                    format: "spoc".to_string(),
                    image_url: "https://example.com/image2.png".to_string(),
                    ranking: SpocRanking {
                        priority: 2,
                        personalization_models: None,
                        item_score: 0.6,
                    },
                    sponsor: "Sponsor2".to_string(),
                    sponsored_by_override: None,
                    title: "Test Title 2".to_string(),
                    url: "https://example2.com".to_string(),
                }),
            ],
        );

        let result = response.filter::<AdSpoc>();

        assert!(result.is_ok());
        let filtered = result.unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered.contains_key("placement_1"));
        assert_eq!(filtered.get("placement_1").unwrap().len(), 2);
    }

    #[test]
    fn test_filter_unexpected_type() {
        let mut response = AdResponse {
            data: HashMap::new(),
        };
        response.data.insert(
            "placement_1".to_string(),
            vec![
                Ad::Image(AdImage {
                    alt_text: None,
                    block_key: "key1".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://example.com/click").unwrap(),
                        impression: Url::parse("https://example.com/impression").unwrap(),
                        report: None,
                    },
                    format: "image".to_string(),
                    image_url: "https://example.com/image.png".to_string(),
                    url: "https://example.com".to_string(),
                }),
                Ad::Spoc(AdSpoc {
                    block_key: "key2".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://example.com/click2").unwrap(),
                        impression: Url::parse("https://example.com/impression2").unwrap(),
                        report: None,
                    },
                    caps: SpocFrequencyCaps {
                        cap_key: "cap2".to_string(),
                        day: 2,
                    },
                    domain: "example2.com".to_string(),
                    excerpt: "Test excerpt 2".to_string(),
                    format: "spoc".to_string(),
                    image_url: "https://example.com/image2.png".to_string(),
                    ranking: SpocRanking {
                        priority: 2,
                        personalization_models: None,
                        item_score: 0.6,
                    },
                    sponsor: "Sponsor2".to_string(),
                    sponsored_by_override: None,
                    title: "Test Title 2".to_string(),
                    url: "https://example2.com".to_string(),
                }),
            ],
        );

        let result = response.filter::<AdImage>();

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            RequestAdsError::UnexpectedAdType {
                placement_id,
                expected_type,
                found_type,
            } => {
                assert_eq!(placement_id, "placement_1");
                assert!(expected_type.contains("AdImage"));
                assert!(found_type.contains("AdSpoc"));
            }
            _ => panic!("Expected UnexpectedAdType error"),
        }
    }

    #[test]
    fn test_filter_and_take_first() {
        let response = get_example_happy_image_response();
        let result = response.filter_and_take_first::<AdImage>();

        assert!(result.is_ok());
        let filtered = result.unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains_key("example_placement_1"));
        assert!(filtered.contains_key("example_placement_2"));
    }
}
