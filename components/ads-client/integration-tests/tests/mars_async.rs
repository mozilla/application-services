use ads_client::{MozAdsClient, MozAdsReportReason, MozAdsTile};
/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/
#[cfg(feature = "stateful")]
use ads_client::{
    ads_store::PlacementId, MozAdType, MozAdsClientBuilder, MozAdsEnvironment, MozAdsIABContent,
    MozAdsIABContentTaxonomy, MozAdsPlacementRequestGeneric, MozAdsRequestOptions,
    MozAdsStoreConfig,
};
#[cfg(feature = "stateful")]
use std::sync::Arc;

#[cfg(feature = "stateful")]
pub const TEST_TIMEOUT_DURATION: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(feature = "stateful")]
fn init_backend() {
    viaduct_hyper::viaduct_init_backend_hyper();
}

#[cfg(feature = "stateful")]
fn prod_client() -> ads_client::MozAdsClient {
    Arc::new(MozAdsClientBuilder::new())
        .environment(MozAdsEnvironment::Prod)
        .store_config(MozAdsStoreConfig {
            db_path: "some_generic_path".to_string(),
            worker_buffer_size: None,
            in_memory: true,
        })
        .build()
}

// Reusable test helper that prefetches a tile ad, waits for background process to complete, and queries it.
// Should mimic the `test_contract_tile_prod_async` test.
fn generate_tile_ad_async_helper(client: &MozAdsClient) -> MozAdsTile {
    // Prefetch
    let placement_id = PlacementId::new("mock_tile_1");
    let result = client.prefetch_ads(
        vec![MozAdsPlacementRequestGeneric {
            iab_content: None,
            placement_id: placement_id.clone(),
            count: None,
            ad_type: MozAdType::Tile,
        }],
        None,
    );

    assert!(
        result.is_ok(),
        "Tile ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // Query
    let result = client.query_tile_ads(placement_id);
    result.expect("`query_tile_ads` in `generate_tile_ad_sync_helper` should return Some")
}

#[cfg(feature = "stateful")]
#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_prod_async() {
    init_backend();

    // Prefetch
    let placement_id = PlacementId::new("mock_billboard_1");
    let client = prod_client();
    let result = client.prefetch_ads(
        vec![MozAdsPlacementRequestGeneric {
            iab_content: None,
            placement_id: placement_id.clone(),
            ad_type: MozAdType::Image,
            count: None,
        }],
        None,
    );

    assert!(
        result.is_ok(),
        "Image ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // Query
    let result = client.query_image_ads(placement_id);
    assert!(result.is_some());
}

#[cfg(feature = "stateful")]
#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_with_categories_prod_async() {
    init_backend();

    // Prefetch
    let placement_id = PlacementId::new("mock_billboard_1");
    let client = prod_client();
    let result = client.prefetch_ads(
        vec![MozAdsPlacementRequestGeneric {
            iab_content: Some(MozAdsIABContent {
                category_ids: vec!["338".to_string()],
                taxonomy: MozAdsIABContentTaxonomy::IAB3_0,
            }),
            count: None,
            ad_type: MozAdType::Image,
            placement_id: placement_id.clone(),
        }],
        Some(MozAdsRequestOptions {
            flags: std::collections::HashMap::from([("contextual_placement".to_string(), true)]),
            ..Default::default()
        }),
    );

    assert!(
        result.is_ok(),
        "Image ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // Query
    let result = client.query_image_ads(placement_id);
    assert!(result.is_some());
    let ad = result.unwrap();
    assert!(!ad.url.is_empty(), "destination url should be populated");
    assert!(!ad.image_url.is_empty(), "image url should be populated");
}

#[cfg(feature = "stateful")]
#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_spoc_prod_async() {
    init_backend();

    // Prefetch
    let placement_id = PlacementId::new("mock_spoc_1");
    let client = prod_client();
    let result = client.prefetch_ads(
        vec![MozAdsPlacementRequestGeneric {
            count: Some(3),
            iab_content: None,
            placement_id: placement_id.clone(),
            ad_type: MozAdType::Spoc,
        }],
        None,
    );

    assert!(
        result.is_ok(),
        "Spoc ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // Query
    let result = client.query_spoc_ads(placement_id);
    assert!(result.is_some());
    assert!(result.unwrap().len() == 3);
}

#[cfg(feature = "stateful")]
#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_tile_prod_async() {
    init_backend();

    // Prefetch
    let placement_id = PlacementId::new("mock_tile_1");
    let client = prod_client();
    let result = client.prefetch_ads(
        vec![MozAdsPlacementRequestGeneric {
            iab_content: None,
            placement_id: placement_id.clone(),
            count: None,
            ad_type: MozAdType::Tile,
        }],
        None,
    );

    assert!(
        result.is_ok(),
        "Tile ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // Query
    let result = client.query_tile_ads(placement_id);
    assert!(result.is_some());
}

#[cfg(feature = "stateful")]
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
    let placement_id = PlacementId::new("mock_tile_1");
    let client = prod_client();
    let result = client.prefetch_ads(
        vec![MozAdsPlacementRequestGeneric {
            iab_content: None,
            placement_id: placement_id.clone(),
            count: None,
            ad_type: MozAdType::Tile,
        }],
        Some(MozAdsRequestOptions {
            ohttp: true,
            ..Default::default()
        }),
    );

    assert!(
        result.is_ok(),
        "Tile ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // Query
    let result = client.query_tile_ads(placement_id);
    assert!(
        result.is_some(),
        "OHTTP response should contain mock_tile_1"
    );
}

#[cfg(feature = "stateful")]
#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_multi_ad_type_prod_async() {
    init_backend();

    // Prefetch
    let placement_image_id = PlacementId::new("mock_billboard_1");
    let placement_spoc_id = PlacementId::new("mock_spoc_1");
    let placement_tile_id = PlacementId::new("mock_tile_1");
    let client = prod_client();
    let result = client.prefetch_ads(
        vec![
            MozAdsPlacementRequestGeneric {
                iab_content: None,
                placement_id: placement_image_id.clone(),
                count: None,
                ad_type: MozAdType::Image,
            },
            MozAdsPlacementRequestGeneric {
                iab_content: None,
                placement_id: placement_spoc_id.clone(),
                count: Some(4),
                ad_type: MozAdType::Spoc,
            },
            MozAdsPlacementRequestGeneric {
                iab_content: None,
                placement_id: placement_tile_id.clone(),
                count: None,
                ad_type: MozAdType::Tile,
            },
        ],
        None,
    );

    assert!(
        result.is_ok(),
        "Image ad dispatch request failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // Query
    let result = client.query_image_ads(placement_image_id);
    assert!(result.is_some());

    let result = client.query_spoc_ads(placement_spoc_id);
    assert!(result.is_some());
    assert!(result.unwrap().len() == 4);

    let result = client.query_tile_ads(placement_tile_id);
    assert!(result.is_some());
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_impression_async() {
    init_backend();

    let client = prod_client();
    let ad = generate_tile_ad_async_helper(&client);

    // Dispatch record_impression asynchronously
    let result = client.record_impression(ad.callbacks.impression.to_string(), None);
    assert!(
        result.is_ok(),
        "record_impression failed: {:?}",
        result.err()
    );

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());
    // TODO: This doesn't actually guarantee the background worker call was successful, doing so requires a callback.
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_click_async() {
    init_backend();
    let client = prod_client();
    let ad = generate_tile_ad_async_helper(&client);

    // Dispatch record_click asynchronously
    let result = client.record_click(ad.callbacks.click.to_string(), None);
    assert!(result.is_ok(), "record_click failed: {:?}", result.err());

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());
    // TODO: This doesn't actually guarantee the background worker call was successful, doing so requires a callback.
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
    let result = client.report_ad(
        report_url.to_string(),
        MozAdsReportReason::NotInterested,
        None,
    );
    assert!(result.is_ok(), "report_ad failed: {:?}", result.err());

    // Ping (waits for queue to clear)
    let ping = client.ping_background_worker(Some(TEST_TIMEOUT_DURATION));
    assert!(ping.is_ok(), "Ping failed: {:?}", ping.err());

    // TODO: This doesn't actually guarantee the background call was successful, doing so requires a callback.
}
