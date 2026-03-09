/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use url::Url;
use viaduct::Request;

use crate::error::BuildRequestError;

#[derive(Debug, PartialEq, Serialize)]
pub struct AdRequest {
    pub context_id: String,
    pub placements: Vec<AdPlacementRequest>,
    /// Skipped to exclude from the request body
    #[serde(skip)]
    pub url: Url,
}

/// Hash implementation intentionally excludes `context_id` as it rotates
/// on client re-instantiation and should not invalidate cached responses.
impl Hash for AdRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url.as_str().hash(state);
        self.placements.hash(state);
    }
}

impl From<AdRequest> for Request {
    fn from(ad_request: AdRequest) -> Self {
        let url = ad_request.url.clone();
        Request::post(url).json(&ad_request)
    }
}

impl AdRequest {
    pub fn try_new(
        context_id: String,
        placements: Vec<AdPlacementRequest>,
        url: Url,
    ) -> Result<Self, BuildRequestError> {
        if placements.is_empty() {
            return Err(BuildRequestError::EmptyConfig);
        };

        let mut request = AdRequest {
            url,
            placements: vec![],
            context_id,
        };

        let mut used_placement_ids: HashSet<String> = HashSet::new();

        for ad_placement_request in placements {
            if used_placement_ids.contains(&ad_placement_request.placement) {
                return Err(BuildRequestError::DuplicatePlacementId {
                    placement_id: ad_placement_request.placement.clone(),
                });
            }

            request.placements.push(AdPlacementRequest {
                placement: ad_placement_request.placement.clone(),
                count: ad_placement_request.count,
                content: ad_placement_request
                    .content
                    .map(|iab_content| AdContentCategory {
                        categories: iab_content.categories,
                        taxonomy: iab_content.taxonomy,
                    }),
            });

            used_placement_ids.insert(ad_placement_request.placement.clone());
        }

        Ok(request)
    }
}

#[derive(Debug, Hash, PartialEq, Serialize)]
pub struct AdPlacementRequest {
    pub placement: String,
    pub count: u32,
    pub content: Option<AdContentCategory>,
}

#[derive(Debug, Deserialize, Hash, PartialEq, Serialize)]
pub struct AdContentCategory {
    pub taxonomy: IABContentTaxonomy,
    pub categories: Vec<String>,
}

#[derive(Debug, Deserialize, Hash, PartialEq, Serialize)]
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

#[cfg(test)]
mod tests {
    use crate::test_utils::TEST_CONTEXT_ID;

    use super::*;
    use serde_json::{json, to_value};

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
    fn test_build_ad_request_happy() {
        let url: Url = "https://example.com/ads".parse().unwrap();
        let request = AdRequest::try_new(
            TEST_CONTEXT_ID.to_string(),
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
                    count: 2,
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB2_1,
                        categories: vec![],
                    }),
                },
            ],
            url.clone(),
        )
        .unwrap();

        let expected_request = AdRequest {
            url,
            context_id: TEST_CONTEXT_ID.to_string(),
            placements: vec![
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
                    count: 2,
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB2_1,
                        categories: vec![],
                    }),
                },
            ],
        };

        assert_eq!(request, expected_request);
    }

    #[test]
    fn test_build_ad_request_fails_on_duplicate_placement_id() {
        let url: Url = "https://example.com/ads".parse().unwrap();
        let request = AdRequest::try_new(
            TEST_CONTEXT_ID.to_string(),
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
                    placement: "example_placement_1".to_string(),
                    count: 1,
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB3_0,
                        categories: vec![],
                    }),
                },
            ],
            url,
        );
        assert!(request.is_err());
    }

    #[test]
    fn test_build_ad_request_fails_on_empty_request() {
        let url: Url = "https://example.com/ads".parse().unwrap();
        let request = AdRequest::try_new(TEST_CONTEXT_ID.to_string(), vec![], url);
        assert!(request.is_err());
    }

    #[test]
    fn test_context_id_ignored_in_hash() {
        use crate::http_cache::RequestHash;

        let url: Url = "https://example.com/ads".parse().unwrap();
        let make_placements = || {
            vec![AdPlacementRequest {
                placement: "tile_1".to_string(),
                count: 1,
                content: None,
            }]
        };

        let context_id_a = "aaaa-bbbb-cccc".to_string();
        let context_id_b = "dddd-eeee-ffff".to_string();

        let req1 = AdRequest::try_new(context_id_a, make_placements(), url.clone()).unwrap();
        let req2 = AdRequest::try_new(context_id_b, make_placements(), url).unwrap();

        assert_eq!(RequestHash::new(&req1), RequestHash::new(&req2));
    }

    /// Contract tests: validate our request serialization against the MARS OpenAPI spec.
    ///
    /// The spec lives at:
    /// https://ads.mozilla.org/assets/docs/openapi/mars-api.html#operation/getAds
    mod contract {
        use super::*;
        use serde_json::{json, to_value};

        #[test]
        fn test_request_serializes_to_spec_shape() {
            let url: Url = "https://ads.mozilla.org/v1/ads".parse().unwrap();
            let request = AdRequest::try_new(
                "decafbad-0cd1-0cd2-0cd3-decafbad1000".to_string(),
                vec![AdPlacementRequest {
                    placement: "newtab_tile_1".to_string(),
                    count: 2,
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB2_1,
                        categories: vec!["technology".to_string()],
                    }),
                }],
                url,
            )
            .unwrap();

            let json = to_value(&request).unwrap();

            // Spec requires: context_id (UUID string), placements (non-empty array)
            assert_eq!(json["context_id"], "decafbad-0cd1-0cd2-0cd3-decafbad1000");
            assert!(json["placements"].is_array());
            assert_eq!(json["placements"][0]["placement"], "newtab_tile_1");
            assert_eq!(json["placements"][0]["count"], 2);
            assert_eq!(json["placements"][0]["content"]["taxonomy"], "IAB-2.1");
            assert_eq!(
                json["placements"][0]["content"]["categories"][0],
                "technology"
            );
            // url must NOT appear in the body (it is sent as the HTTP endpoint)
            assert!(json.get("url").is_none());
        }

        #[test]
        fn test_taxonomy_values_match_spec_strings() {
            // Spec enum: "IAB-1.0" | "IAB-2.0" | "IAB-2.1" | "IAB-2.2" | "IAB-3.0"
            let cases = [
                (IABContentTaxonomy::IAB1_0, "IAB-1.0"),
                (IABContentTaxonomy::IAB2_0, "IAB-2.0"),
                (IABContentTaxonomy::IAB2_1, "IAB-2.1"),
                (IABContentTaxonomy::IAB2_2, "IAB-2.2"),
                (IABContentTaxonomy::IAB3_0, "IAB-3.0"),
            ];
            for (variant, expected) in cases {
                let json = to_value(&variant).unwrap();
                assert_eq!(
                    json,
                    json!(expected),
                    "IABContentTaxonomy variant should serialize to {expected}"
                );
            }
        }

        // INTENTIONAL OMISSION: The MARS spec supports optional `blocks` (array
        // of block_key strings) and `consent` (object with gpp string) fields on
        // the request body. These are not yet modelled because no current embedder
        // needs them. Adding them is additive and non-breaking when needed.
        #[test]
        fn test_optional_spec_fields_not_present_in_serialized_request() {
            let url: Url = "https://ads.mozilla.org/v1/ads".parse().unwrap();
            let request = AdRequest::try_new(
                "decafbad-0cd1-0cd2-0cd3-decafbad1000".to_string(),
                vec![AdPlacementRequest {
                    placement: "newtab_tile_1".to_string(),
                    count: 1,
                    content: None,
                }],
                url,
            )
            .unwrap();

            let json = to_value(&request).unwrap();
            // blocks and consent are not modelled — confirm they are absent
            assert!(json.get("blocks").is_none(), "blocks field should not be serialized");
            assert!(json.get("consent").is_none(), "consent field should not be serialized");
        }
    }

    #[test]
    fn test_different_placements_produce_different_hash() {
        use crate::http_cache::RequestHash;

        let url: Url = "https://example.com/ads".parse().unwrap();

        let req1 = AdRequest::try_new(
            "same-id".to_string(),
            vec![AdPlacementRequest {
                placement: "tile_1".to_string(),
                count: 1,
                content: None,
            }],
            url.clone(),
        )
        .unwrap();

        let req2 = AdRequest::try_new(
            "same-id".to_string(),
            vec![AdPlacementRequest {
                placement: "tile_2".to_string(),
                count: 3,
                content: None,
            }],
            url,
        )
        .unwrap();

        assert_ne!(RequestHash::new(&req1), RequestHash::new(&req2));
    }
}
