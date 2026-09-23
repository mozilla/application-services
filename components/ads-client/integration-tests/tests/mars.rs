/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use ads_client::{
    MozAdsClientBuilder, MozAdsEnvironment, MozAdsIABContent, MozAdsIABContentTaxonomy,
    MozAdsPlacementRequest, MozAdsPlacementRequestWithCount, MozAdsReportReason,
    MozAdsRequestOptions,
};

fn init_backend() {
    viaduct_hyper::viaduct_init_backend_hyper();
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
fn test_contract_image_with_categories_prod() {
    init_backend();

    let client = prod_client();
    let result = client.request_image_ads(
        vec![MozAdsPlacementRequest {
            iab_content: Some(MozAdsIABContent {
                category_ids: vec!["338".to_string()],
                taxonomy: MozAdsIABContentTaxonomy::IAB3_0,
            }),
            placement_id: "mock_billboard_1".to_string(),
        }],
        Some(MozAdsRequestOptions {
            flags: std::collections::HashMap::from([("contextual_placement".to_string(), true)]),
            ..Default::default()
        }),
    );

    assert!(
        result.is_ok(),
        "Image ad request with categories failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();
    let ad = placements
        .get("mock_billboard_1")
        .expect("mock_billboard_1 should be present in the response");
    assert!(!ad.url.is_empty(), "destination url should be populated");
    assert!(!ad.image_url.is_empty(), "image url should be populated");
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

/// Sends the context id deletion request the way `AdsClient` does after a
/// rotation (viaduct OHTTP channel, `DELETE` with a JSON body) and reports
/// how the production edge answers it. `ads_client` keeps its MARS module
/// private, so the request is built here directly.
///
/// This is a write against production, so unlike the read-only contract
/// tests above it does nothing unless `ADS_CLIENT_PROD_DELETE_USER_CONTRACT`
/// is set. CI runs this crate with `--ignored` and must not send it.
///
/// 200 means the request reached MARS; 403 means the edge WAF rejected it.
/// Both are acceptable answers for this contract (the point is that the
/// outcome is known and nothing went out in the clear); anything else means
/// the route or the OHTTP path is broken and must be looked at.
#[test]
#[ignore = "writes to production MARS: run manually with ADS_CLIENT_PROD_DELETE_USER_CONTRACT=1 -- --ignored --nocapture"]
fn test_contract_delete_user_ohttp_prod() {
    if std::env::var_os("ADS_CLIENT_PROD_DELETE_USER_CONTRACT").is_none() {
        eprintln!(
            "skipping: set ADS_CLIENT_PROD_DELETE_USER_CONTRACT=1 to send a DELETE to production"
        );
        return;
    }
    init_backend();
    viaduct::ohttp::configure_ohttp_channel(
        "ads-client".to_string(),
        viaduct::ohttp::OhttpConfig {
            relay_url: "https://mozilla-ohttp.fastly-edge.com/".to_string(),
            gateway_host: "prod.ohttp-gateway.prod.webservices.mozgcp.net".to_string(),
        },
    )
    .expect("OHTTP channel configuration should succeed");

    let settings = viaduct::ClientSettings {
        timeout: 5_000,
        ..viaduct::ClientSettings::default()
    };
    let client = viaduct::Client::with_ohttp_channel("ads-client", settings)
        .expect("ads-client OHTTP channel should be configured");
    let request = viaduct::Request::delete(
        url::Url::parse("https://ads.mozilla.org/v1/delete_user").unwrap(),
    )
    .json(&serde_json::json!({
        // A fixed, never-issued id: MARS treats unknown ids as a no-op.
        "context_id": "00000000-0000-4000-8000-0000000000ac"
    }));

    let response = client
        .send_sync(request)
        .expect("delete_user over OHTTP should get an HTTP response");
    eprintln!(
        "DELETE /v1/delete_user over OHTTP -> {} {}",
        response.status,
        response.text()
    );
    assert!(
        matches!(response.status, 200 | 403),
        "unexpected status {} for delete_user over OHTTP",
        response.status
    );
}
