/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use url::Url;

use crate::client::{
    ad_request::{AdContentCategory, AdPlacementRequest, AdRequest, IABContentTaxonomy},
    ad_response::{
        AdCallbacks, AdImage, AdResponse, AdSpoc, AdTile, SpocFrequencyCaps, SpocRanking,
    },
};

const OPENAPI_JSON: &str = include_str!("../openapi.json");

/// Load a named schema from the MARS OpenAPI spec with `$ref` pointers resolved.
pub fn mars_schema(name: &str) -> serde_json::Value {
    let root: serde_json::Value =
        serde_json::from_str(OPENAPI_JSON).expect("openapi.json should parse");
    let schema = root["components"]["schemas"][name].clone();
    assert!(
        !schema.is_null(),
        "Schema '{name}' not found in openapi.json"
    );
    resolve_refs(&schema, &root)
}

pub fn validate_against_mars_schema(schema: &serde_json::Value, instance: &serde_json::Value) {
    let validator = jsonschema::validator_for(schema).expect("schema should compile");
    if let Err(e) = validator.validate(instance) {
        panic!(
            "JSON Schema validation failed:\n  {e}\nInstance:\n{}",
            serde_json::to_string_pretty(instance).unwrap()
        );
    }
}

fn resolve_refs(value: &serde_json::Value, root: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(ref_path)) = map.get("$ref") {
                let pointer = ref_path.trim_start_matches('#');
                let resolved = root
                    .pointer(pointer)
                    .unwrap_or_else(|| panic!("$ref '{ref_path}' not found in openapi.json"));
                resolve_refs(resolved, root)
            } else {
                let new_map: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), resolve_refs(v, root)))
                    .collect();
                serde_json::Value::Object(new_map)
            }
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| resolve_refs(v, root)).collect())
        }
        other => other.clone(),
    }
}

pub const TEST_CONTEXT_ID: &str = "00000000-0000-4000-8000-000000000001";

pub fn make_happy_placement_requests() -> Vec<AdPlacementRequest> {
    vec![
        AdPlacementRequest {
            placement: "example_placement_1".to_string(),
            count: 1,
            content: Some(AdContentCategory {
                taxonomy: IABContentTaxonomy::IAB2_1,
                categories: vec!["entertainment".to_string()],
            }),
        },
        AdPlacementRequest {
            placement: "example_placement_2".to_string(),
            count: 1,
            content: Some(AdContentCategory {
                taxonomy: IABContentTaxonomy::IAB2_1,
                categories: vec!["entertainment".to_string()],
            }),
        },
    ]
}

pub fn make_happy_ad_request() -> AdRequest {
    let url = format!("{}/ads", mockito::server_url()).parse().unwrap();
    AdRequest::try_new(
        TEST_CONTEXT_ID.to_string(),
        vec![
            AdPlacementRequest {
                placement: "example_placement_1".to_string(),
                count: 1,
                content: None,
            },
            AdPlacementRequest {
                placement: "example_placement_2".to_string(),
                count: 1,
                content: None,
            },
        ],
        url,
    )
    .unwrap()
}

pub fn get_example_happy_image_response() -> AdResponse<AdImage> {
    let base_url = mockito::server_url();
    AdResponse {
        data: HashMap::from([
            (
                "example_placement_1".to_string(),
                vec![AdImage {
                    url: "https://ads.fakeexample.org/example_ad_1".to_string(),
                    image_url: "https://ads.fakeexample.org/example_image_1".to_string(),
                    format: "billboard".to_string(),
                    block_key: "abc123".into(),
                    alt_text: Some("An ad for a puppy".to_string()),
                    callbacks: AdCallbacks {
                        click: Url::parse(&format!("{}/click/example_ad_1", base_url)).unwrap(),
                        impression: Url::parse(&format!("{}/impression/example_ad_1", base_url))
                            .unwrap(),
                        report: Some(
                            Url::parse(&format!("{}/report/example_ad_1", base_url)).unwrap(),
                        ),
                    },
                }],
            ),
            (
                "example_placement_2".to_string(),
                vec![AdImage {
                    url: "https://ads.fakeexample.org/example_ad_2".to_string(),
                    image_url: "https://ads.fakeexample.org/example_image_2".to_string(),
                    format: "skyscraper".to_string(),
                    block_key: "abc123".into(),
                    alt_text: Some("An ad for a pet duck".to_string()),
                    callbacks: AdCallbacks {
                        click: Url::parse(&format!("{}/click/example_ad_2", base_url)).unwrap(),
                        impression: Url::parse(&format!("{}/impression/example_ad_2", base_url))
                            .unwrap(),
                        report: Some(
                            Url::parse(&format!("{}/report/example_ad_2", base_url)).unwrap(),
                        ),
                    },
                }],
            ),
        ]),
    }
}

pub fn get_example_happy_spoc_response() -> AdResponse<AdSpoc> {
    AdResponse {
        data: HashMap::from([
            (
                "example_placement_1".to_string(),
                vec![AdSpoc {
                    url: "https://ads.fakeexample.org/example_spoc_1".to_string(),
                    image_url: "https://ads.fakeexample.org/example_spoc_image_1".to_string(),
                    format: "spoc".to_string(),
                    block_key: "spoc123".into(),
                    title: "Example Spoc Title".to_string(),
                    excerpt: "This is an example spoc excerpt".to_string(),
                    domain: "example.com".to_string(),
                    sponsor: "Example Sponsor".to_string(),
                    sponsored_by_override: None,
                    caps: SpocFrequencyCaps {
                        cap_key: "spoc_cap_1".to_string(),
                        day: 7,
                    },
                    ranking: SpocRanking {
                        priority: 1,
                        personalization_models: Some(HashMap::from([("model1".to_string(), 10)])),
                        item_score: 0.85,
                    },
                    callbacks: AdCallbacks {
                        click: Url::parse("https://ads.fakeexample.org/click/example_spoc_1")
                            .unwrap(),
                        impression: Url::parse(
                            "https://ads.fakeexample.org/impression/example_spoc_1",
                        )
                        .unwrap(),
                        report: Some(
                            Url::parse("https://ads.fakeexample.org/report/example_spoc_1")
                                .unwrap(),
                        ),
                    },
                }],
            ),
            (
                "example_placement_2".to_string(),
                vec![AdSpoc {
                    url: "https://ads.fakeexample.org/example_spoc_2".to_string(),
                    image_url: "https://ads.fakeexample.org/example_spoc_image_2".to_string(),
                    format: "spoc".to_string(),
                    block_key: "spoc456".into(),
                    title: "Another Spoc Title".to_string(),
                    excerpt: "This is another example spoc excerpt".to_string(),
                    domain: "another-example.com".to_string(),
                    sponsor: "Another Sponsor".to_string(),
                    sponsored_by_override: Some("Override Sponsor".to_string()),
                    caps: SpocFrequencyCaps {
                        cap_key: "spoc_cap_2".to_string(),
                        day: 14,
                    },
                    ranking: SpocRanking {
                        priority: 2,
                        personalization_models: None,
                        item_score: 0.75,
                    },
                    callbacks: AdCallbacks {
                        click: Url::parse("https://ads.fakeexample.org/click/example_spoc_2")
                            .unwrap(),
                        impression: Url::parse(
                            "https://ads.fakeexample.org/impression/example_spoc_2",
                        )
                        .unwrap(),
                        report: Some(
                            Url::parse("https://ads.fakeexample.org/report/example_spoc_2")
                                .unwrap(),
                        ),
                    },
                }],
            ),
        ]),
    }
}

pub fn get_example_happy_uatile_response() -> AdResponse<AdTile> {
    AdResponse {
        data: HashMap::from([
            (
                "example_placement_1".to_string(),
                vec![AdTile {
                    url: "https://ads.fakeexample.org/example_uatile_1".to_string(),
                    image_url: "https://ads.fakeexample.org/example_uatile_image_1".to_string(),
                    format: "uatile".to_string(),
                    block_key: "uatile123".into(),
                    name: "Example UA Tile".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://ads.fakeexample.org/click/example_uatile_1")
                            .unwrap(),
                        impression: Url::parse(
                            "https://ads.fakeexample.org/impression/example_uatile_1",
                        )
                        .unwrap(),
                        report: Some(
                            Url::parse("https://ads.fakeexample.org/report/example_uatile_1")
                                .unwrap(),
                        ),
                    },
                }],
            ),
            (
                "example_placement_2".to_string(),
                vec![AdTile {
                    url: "https://ads.fakeexample.org/example_uatile_2".to_string(),
                    image_url: "https://ads.fakeexample.org/example_uatile_image_2".to_string(),
                    format: "uatile".to_string(),
                    block_key: "uatile456".into(),
                    name: "Another UA Tile".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://ads.fakeexample.org/click/example_uatile_2")
                            .unwrap(),
                        impression: Url::parse(
                            "https://ads.fakeexample.org/impression/example_uatile_2",
                        )
                        .unwrap(),
                        report: Some(
                            Url::parse("https://ads.fakeexample.org/report/example_uatile_2")
                                .unwrap(),
                        ),
                    },
                }],
            ),
        ]),
    }
}
