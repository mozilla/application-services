/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;
use ads_client::{
    MozAdsClient, MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest, MozAdsReportReason, MozAdsRequestOptions, worker::ErrorRequestCallback, MozAdsTile,
};
use ads_client::MozAdsIABContentTaxonomy;
use ads_client::MozAdsIABContent;
use ads_client::MozAdsPlacementRequestWithCount;

pub const TEST_TIMEOUT_DURATION : std::time::Duration = std::time::Duration::from_secs(10);

fn init_backend() {
    viaduct_hyper::viaduct_init_backend_hyper();
}

fn prod_client() -> ads_client::MozAdsClient {
    Arc::new(MozAdsClientBuilder::new())
        .environment(MozAdsEnvironment::Prod)
        .build()
}

struct TestErrorCallback;
impl ErrorRequestCallback for TestErrorCallback {
    fn on_error(&self,err: ads_client::MozAdsClientApiError) {
        let ads_client::MozAdsClientApiError::Other { reason } = err;
        panic!("Error received in background worker callback: {:?}", reason)
    }
}

// Reusable helper to prefetches a tile ad, wait for completion, and query it.
// Should mimic the `test_contract_tile_prod_async` test.
fn generate_tile_ad_async_helper(client : &MozAdsClient) -> MozAdsTile {
    // Prefetch
    let placement_id= "mock_tile_1".to_string();
    let result = client.prefetch_ads(        vec![], vec![], vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: placement_id.clone()
        }],
        None, Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Tile ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), Some(Box::new(TestErrorCallback)));
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    // Query
    let result = client.query_tile_ads(placement_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    result.unwrap().expect("`query_tile_ads` in `generate_tile_ad_sync` should return Some")
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_prod_async() {
    init_backend();

    // Prefetch
    let placement_id= "mock_billboard_1".to_string();
    let client = prod_client();
    let result = client.prefetch_ads(vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: placement_id.clone(),
        }], vec![], vec![],
        None, Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Image ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    // Query
    let result = client.query_image_ads(placement_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();

    assert!(placements.is_some());
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_with_categories_prod_async() {
    init_backend();

    // Prefetch
    let placement_id= "mock_billboard_1".to_string();
    let client = prod_client();
    let result = client.prefetch_ads(vec![MozAdsPlacementRequest {
            iab_content: Some(MozAdsIABContent {
                category_ids: vec!["338".to_string()],
                taxonomy: MozAdsIABContentTaxonomy::IAB3_0,
            }),
            placement_id: placement_id.clone()
        }], vec![], vec![],
        Some(MozAdsRequestOptions {
            flags: std::collections::HashMap::from([("contextual_placement".to_string(), true)]),
            ..Default::default()
        }), Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Image ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    // Query
    let result = client.query_image_ads(placement_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    
    let placements = result.unwrap();
    assert!(placements.is_some());
    let ad = placements.unwrap();
    assert!(!ad.url.is_empty(), "destination url should be populated");
    assert!(!ad.image_url.is_empty(), "image url should be populated");
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_spoc_prod_async() {
    init_backend();

    // Prefetch
    let placement_id= "mock_spoc_1".to_string();
    let client = prod_client();
    let result = client.prefetch_ads(vec![], vec![MozAdsPlacementRequestWithCount {
            count: 3,
            iab_content: None,
            placement_id: placement_id.clone(),
        }],  vec![],
        None, Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Spoc ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    // Query
    let result = client.query_spoc_ads(placement_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();
    assert!(placements.is_some());
    assert!(placements.unwrap().len() == 3);
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_tile_prod_async() {
    init_backend();

    // Prefetch
    let placement_id= "mock_tile_1".to_string();
    let client = prod_client();
    let result = client.prefetch_ads(        vec![], vec![], vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: placement_id.clone()
        }],
        None, Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Tile ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    // Query
    let result = client.query_tile_ads(placement_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();

    assert!(placements.is_some());
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_impression_async() {
    init_backend();

    let client = prod_client();
    let ad = generate_tile_ad_async_helper(&client);

    // Dispatch record_impression asynchronously
    let result = client.dispatch_record_impression(ad.callbacks.impression.to_string(), None, Some(Box::new(TestErrorCallback)));
    assert!(
        result.is_ok(),
        "record_impression failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_click_async() {
    init_backend();
    let client = prod_client();
    let ad = generate_tile_ad_async_helper(&client);

    // Dispatch record_click asynchronously
    let result = client.dispatch_record_click(ad.callbacks.click.to_string(), None, Some(Box::new(TestErrorCallback)));
    assert!(result.is_ok(), "record_click failed: {:?}", result.err());

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_report_ad_async() {
    init_backend();

    let client = prod_client();
    let ad = generate_tile_ad_async_helper(&client);

    let report_url = ad
        .callbacks
        .report
        .as_ref()
        .expect("mock_tile_1 should have a report URL");

    let pairs: Vec<(_, _)> = report_url.query_pairs().collect();
    let placement_id_count = pairs.iter().filter(|(k, _)| k == "placement_id").count();
    let position_count = pairs.iter().filter(|(k, _)| k == "position").count();
    assert_eq!(placement_id_count, 1, "expected exactly one placement_id");
    assert_eq!(position_count, 1, "expected exactly one position");

    // Dispatch report_ad asynchronously
    let result = client.dispatch_report_ad(
        report_url.to_string(),
        MozAdsReportReason::NotInterested,
        None,
        Some(Box::new(TestErrorCallback))
    );
    assert!(result.is_ok(), "report_ad failed: {:?}", result.err());

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

}


#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_tile_ohttp_prod_async() {
    init_backend();
    viaduct::ohttp::configure_ohttp_channel(
        "ads-client".to_string(),
        viaduct::ohttp::OhttpConfig {
            relay_url: "https://mozilla-ohttp.fastly-edge.com/".to_string(),
            gateway_host: "prod.ohttp-gateway.prod.webservices.mozgcp.net".to_string(),
        },
    )
    .expect("OHTTP channel configuration should succeed");

    // Prefetch
    let placement_id= "mock_tile_1".to_string();
    let client = prod_client();
    let result = client.prefetch_ads(      vec![],vec![],  vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: placement_id.clone(),
        }],
                    Some(MozAdsRequestOptions {
                ohttp: true,
                ..Default::default()
            }), Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Tile ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    // Query
    let result = client.query_tile_ads(placement_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();

    assert!(placements.is_some(), "OHTTP response should contain mock_tile_1");
}


#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_multi_ad_type_prod_async() {
    init_backend();

    // Prefetch
    let placement_image_id= "mock_billboard_1".to_string();
    let placement_spoc_id= "mock_spoc_1".to_string();
    let placement_tile_id= "mock_tile_1".to_string();
    let client = prod_client();
    let result = client.prefetch_ads(
        vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: placement_image_id.clone(),
        }], vec![MozAdsPlacementRequestWithCount {
            count: 4,
            iab_content: None,
            placement_id: placement_spoc_id.clone(),
        }], vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: placement_tile_id.clone(),
        }],
        None, Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Image ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    // Query
    let result = client.query_image_ads(placement_image_id);
        assert!(
        result.is_ok(),
        "Querying for image ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();
    assert!(placements.is_some());

    let result = client.query_spoc_ads(placement_spoc_id);
        assert!(
        result.is_ok(),
        "Querying for spoc ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();
    assert!(placements.is_some());
    assert!(placements.unwrap().len() == 4);

    let result = client.query_tile_ads(placement_tile_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();

    assert!(placements.is_some());

}