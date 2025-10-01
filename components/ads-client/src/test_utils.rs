/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use crate::{
    mars::DefaultMARSClient,
    models::{AdCallbacks, AdResponse, IABContentTaxonomy, MozAd},
    IABContent, MozAdsPlacement, MozAdsPlacementConfig,
};

pub const TEST_CONTEXT_ID: &str = "00000000-0000-4000-8000-000000000001";

pub fn get_example_happy_placement_config() -> Vec<MozAdsPlacementConfig> {
    vec![
        MozAdsPlacementConfig {
            placement_id: "example_placement_1".to_string(),
            iab_content: Some(IABContent {
                taxonomy: IABContentTaxonomy::IAB2_1,
                category_ids: vec!["entertainment".to_string()],
            }),
        },
        MozAdsPlacementConfig {
            placement_id: "example_placement_2".to_string(),
            iab_content: Some(IABContent {
                taxonomy: IABContentTaxonomy::IAB3_0,
                category_ids: vec![],
            }),
        },
    ]
}

pub fn get_example_happy_ad_response() -> AdResponse {
    AdResponse {
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
                        click: Some("https://ads.fakeexample.org/click/example_ad_1".to_string()),
                        impression: Some(
                            "https://ads.fakeexample.org/impression/example_ad_1".to_string(),
                        ),
                        report: Some("https://ads.fakeexample.org/report/example_ad_1".to_string()),
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
                        click: Some("https://ads.fakeexample.org/click/example_ad_2".to_string()),
                        impression: Some(
                            "https://ads.fakeexample.org/impression/example_ad_2".to_string(),
                        ),
                        report: Some("https://ads.fakeexample.org/report/example_ad_2".to_string()),
                    }),
                }],
            ),
        ]),
    }
}

pub fn get_example_happy_placements() -> HashMap<String, MozAdsPlacement> {
    let mut placements = HashMap::new();
    placements.insert(
        "example_placement_1".to_string(),
        MozAdsPlacement {
            placement_config: MozAdsPlacementConfig {
                placement_id: "example_placement_1".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB2_1,
                    category_ids: vec!["entertainment".to_string()],
                }),
            },
            content: MozAd {
                url: Some("https://ads.fakeexample.org/example_ad_1".to_string()),
                image_url: Some("https://ads.fakeexample.org/example_image_1".to_string()),
                format: Some("billboard".to_string()),
                block_key: None,
                alt_text: Some("An ad for a puppy".to_string()),
                callbacks: Some(AdCallbacks {
                    click: Some("https://ads.fakeexample.org/click/example_ad_1".to_string()),
                    impression: Some(
                        "https://ads.fakeexample.org/impression/example_ad_1".to_string(),
                    ),
                    report: Some("https://ads.fakeexample.org/report/example_ad_1".to_string()),
                }),
            },
        },
    );
    placements.insert(
        "example_placement_2".to_string(),
        MozAdsPlacement {
            placement_config: MozAdsPlacementConfig {
                placement_id: "example_placement_2".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB3_0,
                    category_ids: vec![],
                }),
            },
            content: MozAd {
                url: Some("https://ads.fakeexample.org/example_ad_2".to_string()),
                image_url: Some("https://ads.fakeexample.org/example_image_2".to_string()),
                format: Some("skyscraper".to_string()),
                block_key: None,
                alt_text: Some("An ad for a pet duck".to_string()),
                callbacks: Some(AdCallbacks {
                    click: Some("https://ads.fakeexample.org/click/example_ad_2".to_string()),
                    impression: Some(
                        "https://ads.fakeexample.org/impression/example_ad_2".to_string(),
                    ),
                    report: Some("https://ads.fakeexample.org/report/example_ad_2".to_string()),
                }),
            },
        },
    );
    placements
}

pub fn create_test_client(mock_server_url: String) -> DefaultMARSClient {
    DefaultMARSClient::new_with_endpoint(TEST_CONTEXT_ID.to_string(), mock_server_url)
}
