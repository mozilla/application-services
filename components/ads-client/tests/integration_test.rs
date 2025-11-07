/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::time::Duration;

use ads_client::{
    http_cache::{ByteSize, CacheOutcome, HttpCache, RequestCachePolicy},
    MozAdsClient, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
};
use url::Url;
use viaduct::Request;

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

#[test]
#[ignore]
fn test_cache_works_using_real_timeouts() {
    viaduct_reqwest::use_reqwest_backend();

    let cache = HttpCache::builder("integration_tests.db")
        .default_ttl(Duration::from_secs(60))
        .max_size(ByteSize::mib(1))
        .build()
        .expect("cache build should succeed");

    let url = Url::parse("https://ads.mozilla.org/v1/ads").unwrap();
    let req = Request::post(url).json(&serde_json::json!({
        "context_id": "12347fff-00b0-aaaa-0978-189231239808",
        "placements": [
            {
            "placement": "mock_pocket_billboard_1",
            "count": 1,
            }
        ],
    }));

    let test_ttl = 2;

    // First call: miss -> store
    let o1 = cache
        .send_with_policy(
            &req.clone(),
            &RequestCachePolicy {
                mode: ads_client::http_cache::CacheMode::CacheFirst,
                ttl_seconds: Some(test_ttl),
            },
        )
        .unwrap();
    matches!(o1.cache_outcome, CacheOutcome::MissStored);

    // Second call: hit (no extra HTTP) but no refresh
    let o2 = cache
        .send_with_policy(&req, &RequestCachePolicy::default())
        .unwrap();
    matches!(o2.cache_outcome, CacheOutcome::Hit);
    assert_eq!(o2.response.status, 200);

    // Third call: Miss due to timeout for the test_ttl duration
    std::thread::sleep(Duration::from_secs(test_ttl));
    let o3 = cache
        .send_with_policy(&req, &RequestCachePolicy::default())
        .unwrap();
    matches!(o3.cache_outcome, CacheOutcome::MissStored);
    assert_eq!(o3.response.status, 200);
}
