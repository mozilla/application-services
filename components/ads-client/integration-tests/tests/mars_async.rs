/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{collections::HashMap, sync::{Arc, mpsc::{self, Sender}}};

use ads_client::{
    DispatchCommand, MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest,
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
fn test_contract_tile_prod_async() {
    init_backend();

    let client = prod_client();

    let (success_tx, success_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    pub struct TileCallback {
        on_ad_fn: Box<dyn Fn(HashMap<String,ads_client::MozAdsTile>, Sender<HashMap<String,ads_client::MozAdsTile>>) + Send + Sync>,
        on_error_fn: Box<dyn Fn(ads_client::MozAdsClientApiError, Sender<ads_client::MozAdsClientApiError>) + Send + Sync>,
        success_tx: Sender<HashMap<String,ads_client::MozAdsTile>>,
        err_tx: Sender<ads_client::MozAdsClientApiError>,
    }

    impl ads_client::TileRequestCallback for TileCallback {
        fn on_ad(&self,tiles: std::collections::HashMap<String,ads_client::MozAdsTile>) {
            (self.on_ad_fn)(tiles, self.success_tx.clone())
        }
        fn on_error(&self,err: ads_client::MozAdsClientApiError) {
            (self.on_error_fn)(err, self.err_tx.clone())
        }
    }

    let callback = TileCallback {
        success_tx,
        err_tx,
        on_ad_fn: Box::new(|tiles, tx| {
            tx.send(tiles).expect("Testing channels should not hang up.")
        }),
        on_error_fn: Box::new(|err, tx| {
            tx.send(err).expect("Testing channels should not hang up.")
        })
    };

    client.dispatch(
        DispatchCommand::RequestTileAd { moz_ad_requests: vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: "mock_tile_1".to_string(),
        }], options: None, callback: Arc::new(callback)
    }).expect("Asynchronous dispatch should return Ok()");

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