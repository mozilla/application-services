/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use ads_client::{
    worker::{
        DispatchCommand, ErrorOnlyRequestCallback, ImageRequestCallback, SpocRequestCallback,
        TileRequestCallback,
    },
    MozAdsClient, MozAdsClientApiError, MozAdsClientBuilder, MozAdsEnvironment, MozAdsIABContent,
    MozAdsIABContentTaxonomy, MozAdsImage, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
    MozAdsReportReason, MozAdsRequestOptions, MozAdsSpoc, MozAdsTile,
};
use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, Sender},
        Arc,
    },
};

fn init_backend() {
    viaduct_hyper::viaduct_init_backend_hyper();
}

fn prod_client() -> ads_client::MozAdsClient {
    Arc::new(MozAdsClientBuilder::new())
        .environment(MozAdsEnvironment::Prod)
        .build()
}

// Reusable test structure that implements the varying callback traits with generics.
// We can use a generic here, unlike in MAC, because it's not going through uniffi.
pub struct CallbackTestStruct<T, E> {
    on_ad_fn: Box<dyn Fn(T, Sender<T>) + Send + Sync>,
    on_error_fn: Box<dyn Fn(E, Sender<E>) + Send + Sync>,
    success_tx: Sender<T>,
    err_tx: Sender<E>,
}

impl<T, E> CallbackTestStruct<T, E> {
    pub fn new(success_tx: Sender<T>, err_tx: Sender<E>) -> CallbackTestStruct<T, E> {
        CallbackTestStruct {
            success_tx,
            err_tx,
            on_ad_fn: Box::new(|tiles, tx| {
                tx.send(tiles)
                    .expect("Testing channels should not hang up.")
            }),
            on_error_fn: Box::new(|err, tx| {
                tx.send(err).expect("Testing channels should not hang up.")
            }),
        }
    }
}

impl ErrorOnlyRequestCallback for CallbackTestStruct<(), MozAdsClientApiError> {
    fn on_success(&self) {
        (self.on_ad_fn)((), self.success_tx.clone())
    }
    fn on_error(&self, err: MozAdsClientApiError) {
        (self.on_error_fn)(err, self.err_tx.clone())
    }
}

impl ImageRequestCallback
    for CallbackTestStruct<HashMap<String, ads_client::MozAdsImage>, MozAdsClientApiError>
{
    fn on_ad(&self, ads: HashMap<String, ads_client::MozAdsImage>) {
        (self.on_ad_fn)(ads, self.success_tx.clone())
    }
    fn on_error(&self, err: MozAdsClientApiError) {
        (self.on_error_fn)(err, self.err_tx.clone())
    }
}

impl SpocRequestCallback
    for CallbackTestStruct<HashMap<String, Vec<MozAdsSpoc>>, MozAdsClientApiError>
{
    fn on_ad(&self, ads: HashMap<String, Vec<MozAdsSpoc>>) {
        (self.on_ad_fn)(ads, self.success_tx.clone())
    }
    fn on_error(&self, err: MozAdsClientApiError) {
        (self.on_error_fn)(err, self.err_tx.clone())
    }
}

impl TileRequestCallback for CallbackTestStruct<HashMap<String, MozAdsTile>, MozAdsClientApiError> {
    fn on_ad(&self, tiles: HashMap<String, MozAdsTile>) {
        (self.on_ad_fn)(tiles, self.success_tx.clone())
    }
    fn on_error(&self, err: MozAdsClientApiError) {
        (self.on_error_fn)(err, self.err_tx.clone())
    }
}

// Helper function that runs the tile contract test and produces the result (for tests that require this in setup)
// Callback setup in rust can be weighty for tests, so reuse makes a bit more concise
// This should match the logic of `test_contract_tile_prod_async`
fn generate_ad_test(client: &MozAdsClient) -> HashMap<String, MozAdsTile> {
    // Create ad
    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<HashMap<String, MozAdsTile>, MozAdsClientApiError>::new(
        success_tx, err_tx,
    );

    client
        .dispatch(DispatchCommand::RequestTileAd {
            moz_ad_requests: vec![MozAdsPlacementRequest {
                placement_id: "mock_tile_1".to_string(),
                iab_content: None,
            }],
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    let placements;
    loop {
        if let Ok(placements_res) = success_rx.recv() {
            placements = placements_res;
            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("Tile ad request failed: {:?}", err);
        }
    }

    placements
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_prod_callback() {
    init_backend();
    let client = prod_client();

    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<HashMap<String, MozAdsImage>, MozAdsClientApiError>::new(
        success_tx, err_tx,
    );

    client
        .dispatch(DispatchCommand::RequestImageAds {
            moz_ad_requests: vec![MozAdsPlacementRequest {
                iab_content: None,
                placement_id: "mock_billboard_1".to_string(),
            }],
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(placements) = success_rx.recv() {
            assert!(placements.contains_key("mock_billboard_1"));
            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("Image ad request failed: {:?}", err);
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_with_categories_prod_callback() {
    init_backend();
    let client = prod_client();

    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<HashMap<String, MozAdsImage>, MozAdsClientApiError>::new(
        success_tx, err_tx,
    );

    client
        .dispatch(DispatchCommand::RequestImageAds {
            moz_ad_requests: vec![MozAdsPlacementRequest {
                iab_content: Some(MozAdsIABContent {
                    category_ids: vec!["338".to_string()],
                    taxonomy: MozAdsIABContentTaxonomy::IAB3_0,
                }),
                placement_id: "mock_billboard_1".to_string(),
            }],
            options: Some(MozAdsRequestOptions {
                flags: std::collections::HashMap::from([(
                    "contextual_placement".to_string(),
                    true,
                )]),
                ..Default::default()
            }),
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(placements) = success_rx.recv() {
            assert!(placements.contains_key("mock_billboard_1"));
            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("Image ad request failed: {:?}", err);
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_spoc_prod_callback() {
    init_backend();
    let client = prod_client();

    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback =
        CallbackTestStruct::<HashMap<String, Vec<MozAdsSpoc>>, MozAdsClientApiError>::new(
            success_tx, err_tx,
        );

    client
        .dispatch(DispatchCommand::RequestSpocAds {
            moz_ad_requests: vec![MozAdsPlacementRequestWithCount {
                count: 3,
                iab_content: None,
                placement_id: "mock_spoc_1".to_string(),
            }],
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(placements) = success_rx.recv() {
            assert!(placements.contains_key("mock_spoc_1"));
            assert!(placements.get("mock_spoc_1").unwrap().len() == 3);

            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("Spoc ad request failed: {:?}", err);
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_tile_prod_callback() {
    init_backend();
    let client = prod_client();

    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<HashMap<String, MozAdsTile>, MozAdsClientApiError>::new(
        success_tx, err_tx,
    );

    client
        .dispatch(DispatchCommand::RequestTileAd {
            moz_ad_requests: vec![MozAdsPlacementRequest {
                iab_content: None,
                placement_id: "mock_tile_1".to_string(),
            }],
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(placements) = success_rx.recv() {
            assert!(placements.contains_key("mock_tile_1"));
            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("Tile ad request failed: {:?}", err);
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_impression_callback() {
    init_backend();
    let client = prod_client();

    // Generate tiles
    let placements = generate_ad_test(&client);
    let ad = placements
        .get("mock_tile_1")
        .clone()
        .expect("mock_tile_1 placement should be present");

    // Record an impression
    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<(), MozAdsClientApiError>::new(success_tx, err_tx);

    client
        .dispatch(DispatchCommand::RecordImpression {
            impression_url: ad.callbacks.impression.to_string(),
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(_) = success_rx.recv() {
            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("record_impression failed: {:?}", err);
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_record_click_callback() {
    init_backend();
    let client = prod_client();

    // Generate tiles
    let placements = generate_ad_test(&client);
    let ad = placements
        .get("mock_tile_1")
        .clone()
        .expect("mock_tile_1 placement should be present");

    // Record a click
    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<(), MozAdsClientApiError>::new(success_tx, err_tx);

    client
        .dispatch(DispatchCommand::RecordClick {
            click_url: ad.callbacks.click.to_string(),
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(_) = success_rx.recv() {
            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("record_click failed: {:?}", err);
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_report_ad_callback() {
    init_backend();
    let client = prod_client();

    // Generate tiles
    let placements = generate_ad_test(&client);
    let ad = placements
        .get("mock_tile_1")
        .clone()
        .expect("mock_tile_1 placement should be present");

    // Assertions on the ad
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

    // Report the ad
    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<(), MozAdsClientApiError>::new(success_tx, err_tx);

    client
        .dispatch(DispatchCommand::ReportAd {
            report_url: report_url.to_string(),
            reason: MozAdsReportReason::NotInterested,
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(_) = success_rx.recv() {
            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("report_ad failed: {:?}", err);
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_tile_ohttp_prod_callback() {
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

    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let callback = CallbackTestStruct::<HashMap<String, MozAdsTile>, MozAdsClientApiError>::new(
        success_tx, err_tx,
    );

    client
        .dispatch(DispatchCommand::RequestTileAd {
            moz_ad_requests: vec![MozAdsPlacementRequest {
                placement_id: "mock_tile_1".to_string(),
                iab_content: None,
            }],
            options: None,
            callback: Box::new(callback),
        })
        .expect("Asynchronous dispatch should return Ok()");

    loop {
        if let Ok(placements) = success_rx.recv() {
            assert!(
                placements.contains_key("mock_tile_1"),
                "OHTTP response should contain mock_tile_1"
            );

            break;
        }
        if let Ok(err) = err_rx.recv() {
            panic!("Tile ad request over OHTTP should succeed: {:?}", err);
        }
    }
}
