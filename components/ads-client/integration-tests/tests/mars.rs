/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use ads_client::{
    MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest,
    MozAdsPlacementRequestWithCount, MozAdsReportReason, MozAdsRequestOptions,
};

fn init_backend() {
    // Err means the backend is already initialized.
    let _ = viaduct_hyper::viaduct_init_backend_hyper();
}

fn prod_client() -> ads_client::MozAdsClient {
    Arc::new(MozAdsClientBuilder::new())
        .environment(MozAdsEnvironment::Prod)
        .build()
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_prod() {
    init_backend();

    let client = prod_client();
    let result = client.request_image_ads(
        vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: "mock_billboard_1".to_string(),
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
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_spoc_prod() {
    init_backend();

    let client = prod_client();
    let result = client.request_spoc_ads(
        vec![MozAdsPlacementRequestWithCount {
            count: 3,
            iab_content: None,
            placement_id: "mock_spoc_1".to_string(),
        }],
        None,
    );

    assert!(result.is_ok(), "Spoc ad request failed: {:?}", result.err());
    let placements = result.unwrap();
    assert!(placements.contains_key("mock_spoc_1"));
    assert!(placements.get("mock_spoc_1").unwrap().len() == 3);
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_tile_prod() {
    init_backend();

    let client = prod_client();
    let result = client.request_tile_ads(
        vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: "mock_tile_1".to_string(),
        }],
        None,
    );

    assert!(result.is_ok(), "Tile ad request failed: {:?}", result.err());
    let placements = result.unwrap();
    assert!(placements.contains_key("mock_tile_1"));
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_impression() {
    init_backend();

    let client = prod_client();
    let placements = client
        .request_tile_ads(
            vec![MozAdsPlacementRequest {
                placement_id: "mock_tile_1".to_string(),
                iab_content: None,
            }],
            None,
        )
        .expect("tile ad request should succeed");

    let ad = placements
        .get("mock_tile_1")
        .expect("mock_tile_1 placement should be present");

    let result = client.record_impression(ad.callbacks.impression.to_string(), None);
    assert!(
        result.is_ok(),
        "record_impression failed: {:?}",
        result.err()
    );
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_click() {
    init_backend();

    let client = prod_client();
    let placements = client
        .request_tile_ads(
            vec![MozAdsPlacementRequest {
                placement_id: "mock_tile_1".to_string(),
                iab_content: None,
            }],
            None,
        )
        .expect("tile ad request should succeed");

    let ad = placements
        .get("mock_tile_1")
        .expect("mock_tile_1 placement should be present");

    let result = client.record_click(ad.callbacks.click.to_string(), None);
    assert!(result.is_ok(), "record_click failed: {:?}", result.err());
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_report_ad() {
    init_backend();

    let client = prod_client();
    let placements = client
        .request_tile_ads(
            vec![MozAdsPlacementRequest {
                placement_id: "mock_tile_1".to_string(),
                iab_content: None,
            }],
            None,
        )
        .expect("tile ad request should succeed");

    let ad = placements
        .get("mock_tile_1")
        .expect("mock_tile_1 placement should be present");

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

    let result = client.report_ad(
        report_url.to_string(),
        MozAdsReportReason::NotInterested,
        None,
    );
    assert!(result.is_ok(), "report_ad failed: {:?}", result.err());
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_tile_ohttp_prod() {
    init_backend();
    viaduct::ohttp::configure_ohttp_channel(
        "ads-client".to_string(),
        viaduct::ohttp::OhttpConfig {
            relay_url: "https://mozilla-ohttp.fastly-edge.com/".to_string(),
            gateway_host: "prod.ohttp-gateway.prod.webservices.mozgcp.net".to_string(),
        },
    )
    .expect("OHTTP channel configuration should succeed");

    let client = prod_client();

    let placements = client
        .request_tile_ads(
            vec![MozAdsPlacementRequest {
                iab_content: None,
                placement_id: "mock_tile_1".to_string(),
            }],
            Some(MozAdsRequestOptions {
                ohttp: true,
                ..Default::default()
            }),
        )
        .expect("tile ad request over OHTTP should succeed");
    assert!(
        placements.contains_key("mock_tile_1"),
        "OHTTP response should contain mock_tile_1"
    );
}
