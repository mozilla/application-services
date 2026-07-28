/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use ads_client::{
    worker::{
        DispatchCommand, ErrorOnlyRequestCallback, ImageRequestCallback, SpocRequestCallback,
        TileRequestCallback,
    },
    MozAdsClientApiError, MozAdsClientBuilder, MozAdsEnvironment, MozAdsIABContent,
    MozAdsIABContentTaxonomy, MozAdsImage, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
    MozAdsRequestOptions, MozAdsSpoc, MozAdsTile,
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

// TODO: explain
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

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_prod_async() {
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
            callback: Arc::new(callback),
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
fn test_contract_image_with_categories_prod_async() {
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
            callback: Arc::new(callback),
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
fn test_contract_spoc_prod_async() {
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
            callback: Arc::new(callback),
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
fn test_contract_tile_prod_async() {
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
            callback: Arc::new(callback),
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
