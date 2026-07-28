use std::{collections::HashMap, sync::{Arc, mpsc::Receiver}};
use error_support::handle_error;

use crate::{AdsClientApiResult, MozAdsClientApiError, MozAdsClientInner, MozAdsPlacementRequest, MozAdsRequestOptions, MozAdsTile, client::error::ComponentError, http_cache::CachePolicy, mars::ad_request::{AdPlacementRequest, AdRequestFlags}};

pub fn worker(inner_client : MozAdsClientInner, rx : Receiver<DispatchCommand>) {
    // TODO: This could be an async environment where we wait on buffered futures- can send many out at once, instead of FIFO.
    while let Ok(task) = rx.recv() {
        let task : DispatchCommand = task;
        task.run_command(&inner_client).expect("TODO: handle error here. maybe retries should be here?");
    }
}

#[derive(uniffi::Enum)]

pub enum DispatchCommand {
    RequestTileAd {
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
        callback: Arc<dyn TileRequestCallback>,
    }
}

impl DispatchCommand {

    #[handle_error(ComponentError)]
    pub fn run_command(self, ads_client_inner : &MozAdsClientInner) -> AdsClientApiResult<()> {
        // TODO: Duplicated behavior with the sync functions- either they call this or you call those
        match self {
            DispatchCommand::RequestTileAd { moz_ad_requests, options, callback } => {
                let inner = ads_client_inner.lock();
                let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
                let options = options.unwrap_or_default();
                let flags = AdRequestFlags::from(&options);
                let ohttp = options.ohttp;
                let cache_policy: CachePolicy = options.into();
                let response = inner
                    .request_tile_ads(requests, flags, Some(cache_policy), ohttp)
                    .map_err(ComponentError::RequestAds)?;
                let tiles = response.into_iter().map(|(k, v)| (k, v.into())).collect();
                callback.on_ad(tiles);
                // TODO: error, modular version
            }
        }
        Ok(())
    }
}

#[uniffi::export(with_foreign)]
pub trait TileRequestCallback : Send + Sync {
    fn on_ad(&self, tiles : HashMap<String, MozAdsTile>);
    fn on_error(&self, err: MozAdsClientApiError);
}
