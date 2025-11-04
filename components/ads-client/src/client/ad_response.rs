/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::instrument::{emit_telemetry_event, TelemetryEvent};
use serde::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct AdResponse {
    #[serde(deserialize_with = "AdResponse::deserialize_ad_response", flatten)]
    pub data: HashMap<String, Vec<MozAd>>,
}

impl AdResponse {
    fn deserialize_ad_response<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<String, Vec<MozAd>>, D::Error>
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
pub struct AdCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}

#[cfg(test)]
mod tests {
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
                vec![MozAd {
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
