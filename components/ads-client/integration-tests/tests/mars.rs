/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use ads_client::{
    MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
};

fn init_backend() {
    // Err means the backend is already initialized.
    let _ = viaduct_hyper::viaduct_init_backend_hyper();
}

fn staging_client() -> ads_client::MozAdsClient {
    Arc::new(MozAdsClientBuilder::new())
        .environment(MozAdsEnvironment::Staging)
        .build()
}

#[test]
fn test_contract_image_staging() {
    init_backend();

    let client = staging_client();
    let result = client.request_image_ads(
        vec![MozAdsPlacementRequest {
            placement_id: "mock_billboard_1".to_string(),
            iab_content: None,
        }],
        None,
    );

    assert!(
        result.is_ok(),
        "Image ad request failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();
    assert!(placements.contains_key("mock_billboard_1"));
}

#[test]
fn test_contract_spoc_staging() {
    init_backend();

    let client = staging_client();
    let result = client.request_spoc_ads(
        vec![MozAdsPlacementRequestWithCount {
            placement_id: "mock_spoc_1".to_string(),
            count: 3,
            iab_content: None,
        }],
        None,
    );

    assert!(result.is_ok(), "Spoc ad request failed: {:?}", result.err());
    let placements = result.unwrap();
    assert!(placements.contains_key("mock_spoc_1"));
    assert!(placements.get("mock_spoc_1").unwrap().len() == 3);
}

#[test]
fn test_contract_tile_staging() {
    init_backend();

    let client = staging_client();
    let result = client.request_tile_ads(
        vec![MozAdsPlacementRequest {
            placement_id: "mock_tile_1".to_string(),
            iab_content: None,
        }],
        None,
    );

    assert!(result.is_ok(), "Tile ad request failed: {:?}", result.err());
    let placements = result.unwrap();
    assert!(placements.contains_key("mock_tile_1"));
}
