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

#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Enum)]
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

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Deserialize, uniffi::Enum)]
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

#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct AdContentCategory {
    pub taxonomy: IABContentTaxonomy,
    pub categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct AdRequest {
    pub context_id: String,
    pub placements: Vec<AdPlacementRequest>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct AdCallbacks {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub click: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub impression: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub report: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct MozAd {
    pub alt_text: Option<String>,
    pub block_key: Option<String>,
    pub callbacks: Option<AdCallbacks>,
    pub format: Option<String>,
    pub image_url: Option<String>, //TODO: Consider if we want to load the image locally
    pub url: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct AdResponse {
    #[serde(flatten)]
    pub data: HashMap<String, Vec<MozAd>>,
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
    fn test_ad_callbacks_empty_and_missing_to_none() {
        // empty strings, whitespace, or missing fields should become None.
        let j = json!({
            "click": "",
            "impression": "   ",
            // "report" omitted
        })
        .to_string();

        let got: AdCallbacks = from_str(&j).unwrap();
        assert_eq!(
            got,
            AdCallbacks {
                click: None,
                impression: None,
                report: None
            }
        );
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
                block_key: Some("abc123".into()),
                callbacks: Some(AdCallbacks {
                    click: Some("https://buyanvilseveryday.test/click".into()),
                    impression: Some("https://buyanvilseveryday.test/impression".into()),
                    report: Some("https://buyanvilseveryday.test/report".into()),
                }),
                format: Some("Leaderboard".into()),
                image_url: Some("https://buyanvilseveryday.test/img.png".into()),
                url: Some("https://buyanvilseveryday.test".into()),
            }
        );
    }

    #[test]
    fn test_moz_ad_response_partial() {
        let response_partial = json!({
            "alt_text": null,
            "callbacks": {
                "click": "",
                "impression": "   ",
                "report": null
            }
        })
        .to_string();

        let partial: MozAd = from_str(&response_partial).unwrap();
        assert_eq!(
            partial,
            MozAd {
                alt_text: None,
                block_key: None,
                callbacks: Some(AdCallbacks {
                    click: None,
                    impression: None,
                    report: None,
                }),
                format: None,
                image_url: None,
                url: None,
            }
        );
    }

    #[test]
    fn test_ad_response_serialization() {
        let raw_ad_response = json!({
            "example_placement_1": [
                {
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
            data: HashMap::from([
                (
                    "example_placement_1".to_string(),
                    vec![MozAd {
                        url: Some("https://ads.fakeexample.org/example_ad_1".to_string()),
                        image_url: Some("https://ads.fakeexample.org/example_image_1".to_string()),
                        format: Some("billboard".to_string()),
                        block_key: None,
                        alt_text: Some("An ad for a puppy".to_string()),
                        callbacks: Some(AdCallbacks {
                            click: Some(
                                "https://ads.fakeexample.org/click/example_ad_1".to_string(),
                            ),
                            impression: None, // Missing impression callback URL
                            report: Some(
                                "https://ads.fakeexample.org/report/example_ad_1".to_string(),
                            ),
                        }),
                    }],
                ),
                (
                    "example_placement_2".to_string(),
                    vec![MozAd {
                        url: Some("https://ads.fakeexample.org/example_ad_2".to_string()),
                        image_url: Some("https://ads.fakeexample.org/example_image_2".to_string()),
                        format: Some("skyscraper".to_string()),
                        block_key: None,
                        alt_text: Some("An ad for a pet duck".to_string()),
                        callbacks: Some(AdCallbacks {
                            click: Some(
                                "https://ads.fakeexample.org/click/example_ad_2".to_string(),
                            ),
                            impression: Some(
                                "https://ads.fakeexample.org/impression/example_ad_2".to_string(),
                            ),
                            report: Some(
                                "https://ads.fakeexample.org/report/example_ad_2".to_string(),
                            ),
                        }),
                    }],
                ),
            ]),
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
            data: HashMap::from([
                ("example_placement_1".to_string(), vec![]),
                ("example_placement_2".to_string(), vec![]),
            ]),
        };

        assert_eq!(parsed, expected);
    }
}
