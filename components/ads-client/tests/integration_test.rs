/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use ads_client::{MozAdsClient, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount};

#[test]
#[ignore]
fn test_mock_pocket_billboard_1_placement() {
    viaduct_reqwest::use_reqwest_backend();

    let client = MozAdsClient::new(None);

    let placement_request = MozAdsPlacementRequest {
        placement_id: "mock_pocket_billboard_1".to_string(),
        iab_content: None,
    };

    let result = client.request_ads(vec![placement_request], None);
    println!("result: {:?}", result);

    assert!(result.is_ok(), "Failed to request ads: {:?}", result.err());

    let placements = result.unwrap();

    assert!(
        placements.contains_key("mock_pocket_billboard_1"),
        "Response should contain placement_id 'mock_pocket_billboard_1'"
    );

    let placement = placements
        .get("mock_pocket_billboard_1")
        .expect("Placement should exist");

    assert!(!placement.url.is_empty(), "Ad URL should not be empty");
    assert!(
        !placement.image_url.is_empty(),
        "Ad image URL should not be empty"
    );
    assert!(
        !placement.format.is_empty(),
        "Ad format should not be empty"
    );
    assert!(
        !placement.block_key.is_empty(),
        "Ad block_key should not be empty"
    );
}

#[test]
#[ignore]
fn test_request_ads_multiset_count() {
    viaduct_reqwest::use_reqwest_backend();

    let client = MozAdsClient::new(None);

    let requested_count = 3;
    let placement_request = MozAdsPlacementRequestWithCount {
        placement_id: "mock_pocket_billboard_1".to_string(),
        count: requested_count,
        iab_content: None,
    };

    let result = client.request_ads_multiset(vec![placement_request], None);
    println!("result: {:?}", result);

    assert!(result.is_ok(), "Failed to request ads: {:?}", result.err());

    let placements = result.unwrap();

    assert!(
        placements.contains_key("mock_pocket_billboard_1"),
        "Response should contain placement_id 'mock_pocket_billboard_1'"
    );

    let ads = placements
        .get("mock_pocket_billboard_1")
        .expect("Placement should exist");

    assert_eq!(
        ads.len(),
        requested_count as usize,
        "Should have {} ads, but got {}",
        requested_count,
        ads.len()
    );

    for (index, ad) in ads.iter().enumerate() {
        assert!(!ad.url.is_empty(), "Ad {} URL should not be empty", index);
        assert!(
            !ad.image_url.is_empty(),
            "Ad {} image URL should not be empty",
            index
        );
        assert!(
            !ad.format.is_empty(),
            "Ad {} format should not be empty",
            index
        );
        assert!(
            !ad.block_key.is_empty(),
            "Ad {} block_key should not be empty",
            index
        );
    }
}
