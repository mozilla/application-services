/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::instrument::{emit_telemetry_event, TelemetryEvent};

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct AdPlacementRequest {
    pub placement: String,
    pub count: u32,
    pub content: Option<AdContentCategory>,
}

#[derive(Debug, Deserialize, uniffi::Enum, PartialEq, Serialize)]
pub enum IABAdUnitFormat {
    Billboard,
    SmartphoneBanner300,
    SmartphoneBanner320,
    Leaderboard,
    SuperLeaderboardPushdown,
    Portrait,
    Skyscraper,
    MediumRectangle,
    TwentyBySixty,
    MobilePhoneInterstitial640,
    MobilePhoneInterstitial750,
    MobilePhoneInterstitial1080,
    FeaturePhoneSmallBanner,
    FeaturePhoneMediumBanner,
    FeaturePhoneLargeBanner,
}

#[derive(Clone, Copy, Debug, Deserialize, uniffi::Enum, PartialEq, Serialize)]
pub enum IABContentTaxonomy {
    #[serde(rename = "IAB-1.0")]
    IAB1_0,

    #[serde(rename = "IAB-2.0")]
    IAB2_0,

    #[serde(rename = "IAB-2.1")]
    IAB2_1,

    #[serde(rename = "IAB-2.2")]
    IAB2_2,

    #[serde(rename = "IAB-3.0")]
    IAB3_0,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct AdContentCategory {
    pub taxonomy: IABContentTaxonomy,
    pub categories: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct AdRequest {
    pub context_id: String,
    pub placements: Vec<AdPlacementRequest>,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct AdCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct MozAd {
    pub alt_text: Option<String>,
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub format: String,
    pub image_url: String, //TODO: Consider if we want to load the image locally
    pub url: String,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct AdResponse {
    #[serde(deserialize_with = "deserialize_ad_response", flatten)]
    pub data: HashMap<String, Vec<MozAd>>,
}

fn deserialize_ad_response<'de, D>(deserializer: D) -> Result<HashMap<String, Vec<MozAd>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = HashMap::<String, serde_json::Value>::deserialize(deserializer)?;
    let mut result = HashMap::new();

    for (key, value) in raw {
        if let serde_json::Value::Array(arr) = value {
            let ads: Vec<MozAd> = arr
                .into_iter()
                .filter_map(|item| match serde_json::from_value::<MozAd>(item) {
                    Ok(ad) => Some(ad),
                    Err(_) => {
                        // TODO: improve the telemetry event (should we include the invalid URL?)
                        let _ = emit_telemetry_event(Some(TelemetryEvent::InvalidUrlError));
                        None
                    }
                })
                .collect();
            if !ads.is_empty() {
                result.insert(key, ads);
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, json, to_value};

    #[test]
    fn test_ad_placement_request_with_content_serialize() {
        let request = AdPlacementRequest {
            placement: "example_placement".into(),
            count: 1,
            content: Some(AdContentCategory {
                taxonomy: IABContentTaxonomy::IAB2_1,
                categories: vec!["Technology".into(), "Programming".into()],
            }),
        };

        let serialized = to_value(&request).unwrap();

        let expected_json = json!({
            "placement": "example_placement",
            "count": 1,
            "content": {
                "taxonomy": "IAB-2.1",
                "categories": ["Technology", "Programming"]
            }
        });

        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_iab_content_taxonomy_serialize() {
        use serde_json::to_string;

        // We expect that enums map to strings like "IAB-2.2"
        let s = to_string(&IABContentTaxonomy::IAB1_0).unwrap();
        assert_eq!(s, "\"IAB-1.0\"");

        let s = to_string(&IABContentTaxonomy::IAB2_0).unwrap();
        assert_eq!(s, "\"IAB-2.0\"");

        let s = to_string(&IABContentTaxonomy::IAB2_1).unwrap();
        assert_eq!(s, "\"IAB-2.1\"");

        let s = to_string(&IABContentTaxonomy::IAB2_2).unwrap();
        assert_eq!(s, "\"IAB-2.2\"");

        let s = to_string(&IABContentTaxonomy::IAB3_0).unwrap();
        assert_eq!(s, "\"IAB-3.0\"");
    }

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

        let full: MozAd = from_str(&response_full).unwrap();
        assert_eq!(
            full,
            MozAd {
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
            }
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

        let partial: MozAd = from_str(&response_partial).unwrap();
        assert_eq!(
            partial,
            MozAd {
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
            }
        );
    }

    #[test]
    fn test_ad_response_serialization() {
        let raw_ad_response = json!({
            "example_placement_1": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_1",
                    "image_url": "https://ads.fakeexample.org/example_image_1",
                    "format": "billboard",
                    "alt_text": "An ad for a puppy",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_1",
                        // impression is intentionally missing
                        "report": "https://ads.fakeexample.org/report/example_ad_1"
                    }
                }
            ],
            "example_placement_2": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_2",
                    "image_url": "https://ads.fakeexample.org/example_image_2",
                    "format": "skyscraper",
                    "alt_text": "An ad for a pet duck",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_2",
                        "impression": "https://ads.fakeexample.org/impression/example_ad_2",
                        "report": "https://ads.fakeexample.org/report/example_ad_2"
                    }
                }
            ]
        })
        .to_string();

        let parsed: AdResponse = from_str(&raw_ad_response).unwrap();

        let expected = AdResponse {
            data: HashMap::from([(
                "example_placement_2".to_string(),
                vec![MozAd {
                    url: "https://ads.fakeexample.org/example_ad_2".to_string(),
                    image_url: "https://ads.fakeexample.org/example_image_2".to_string(),
                    format: "skyscraper".to_string(),
                    block_key: "abc123".into(),
                    alt_text: Some("An ad for a pet duck".to_string()),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://ads.fakeexample.org/click/example_ad_2")
                            .unwrap(),
                        impression: Url::parse(
                            "https://ads.fakeexample.org/impression/example_ad_2",
                        )
                        .unwrap(),
                        report: Some(
                            Url::parse("https://ads.fakeexample.org/report/example_ad_2").unwrap(),
                        ),
                    },
                }],
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
}
