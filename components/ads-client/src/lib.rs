/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[cfg(feature = "stateful")]
use crate::{
    ads_store::PlacementId, client::error::BackgroundWorkerError, worker::command::DispatchCommand,
    worker::BackgroundWorker,
};
use client::error::ComponentError;
use error_support::handle_error;
use mars::error::CallbackRequestError;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
#[cfg(feature = "stateful")]
use std::{sync::mpsc, time::Duration};
use url::Url as AdsClientUrl;

use client::AdsClient;
use http_cache::CachePolicy;
use mars::ad_request::{AdPlacementRequest, AdRequestFlags};
#[cfg(feature = "stateful")]
pub mod ads_store;
mod client;
pub mod common;
mod ffi;
pub mod http_cache;
mod mars;
pub mod shutdown;
pub mod telemetry;
#[cfg(feature = "stateful")]
pub mod worker;

pub use ffi::*;

use crate::{ffi::telemetry::MozAdsTelemetryWrapper, shutdown::ShutdownReferences};

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

uniffi::custom_type!(AdsClientUrl, String, {
    remote,
    try_lift: |val| Ok(AdsClientUrl::parse(&val)?),
    lower: |obj| obj.as_str().to_string(),
});

#[cfg(feature = "stateful")]
uniffi::custom_type!(PlacementId, String);

pub type MozAdsClientInner = Arc<Mutex<AdsClient<MozAdsTelemetryWrapper>>>;
#[derive(uniffi::Object)]
pub struct MozAdsClient {
    inner: MozAdsClientInner,
    shutdown_references: ShutdownReferences<MozAdsTelemetryWrapper>,
    #[cfg(feature = "stateful")]
    worker: BackgroundWorker,
}

#[uniffi::export]
impl MozAdsClient {
    pub fn clear_cache(&self) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .clear_cache()
            .map_err(|e| MozAdsClientApiError::Other {
                reason: format!("Failed to clear cache: {}", e),
            })
    }

    // Allows the ads-client to unload some references and prepare for a safe shutdown.
    // Other methods should not be called after this one.
    // Currently, we attempt to shutdown and log any errors instead of returning them.
    // However, we may yet want to do so, so we keep the Result.
    #[uniffi::method()]
    pub fn shutdown(&self) -> AdsClientApiResult<()> {
        if let Err(e) = self.shutdown_references.shutdown() {
            error_support::error!("Could not successfully shutdown ads-client: {e}");
        }
        Ok(())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_image_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsImage>> {
        let inner = self.inner.lock();
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = inner
            .request_image_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_spoc_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAdsSpoc>>> {
        let inner = self.inner.lock();
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = inner
            .request_spoc_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds)?;
        Ok(response
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|spoc| spoc.into()).collect()))
            .collect())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_tile_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsTile>> {
        let inner = self.inner.lock();
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = inner
            .request_tile_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

#[cfg(not(feature = "stateful"))]
#[uniffi::export]
impl MozAdsClient {
    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_click(
        &self,
        click_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&click_url)
            .map_err(|e| ComponentError::RecordClick(CallbackRequestError::InvalidUrl(e).into()))?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let inner = self.inner.lock();
        inner
            .record_click(url, ohttp)
            .map_err(ComponentError::RecordClick)
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_impression(
        &self,
        impression_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&impression_url).map_err(|e| {
            ComponentError::RecordImpression(CallbackRequestError::InvalidUrl(e).into())
        })?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let inner = self.inner.lock();
        inner
            .record_impression(url, ohttp)
            .map_err(ComponentError::RecordImpression)
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn report_ad(
        &self,
        report_url: String,
        reason: MozAdsReportReason,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&report_url)
            .map_err(|e| ComponentError::ReportAd(CallbackRequestError::InvalidUrl(e).into()))?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let inner = self.inner.lock();
        inner
            .report_ad(url, reason.into(), ohttp)
            .map_err(ComponentError::ReportAd)
    }
}

#[cfg(feature = "stateful")]
#[uniffi::export]
impl MozAdsClient {
    // TODO: Can we make this one request?
    #[handle_error(ComponentError)]
    #[uniffi::method(default(ad_requests = [], options = None))]
    pub fn prefetch_ads(
        &self,
        ad_requests: Vec<MozAdsPlacementRequestGeneric>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<()> {
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let blocks = options.blocks.clone();
        let cache_policy: CachePolicy = options.into();

        // Sort the ads to batch them into separate requests.
        // TODO: We will refactor some of the MARS backend to be able to do this in one request, and therefore not need sorting, nor will it need this conversion away from a central type.
        let mut image_ad_requests = vec![];
        let mut spoc_ad_requests = vec![];
        let mut tile_ad_requests = vec![];
        for ad in ad_requests {
            match ad.ad_type {
                MozAdType::Image => image_ad_requests.push(MozAdsPlacementRequest {
                    iab_content: ad.iab_content,
                    placement_id: ad.placement_id.into(),
                }),
                MozAdType::Spoc => spoc_ad_requests.push(MozAdsPlacementRequestWithCount {
                    iab_content: ad.iab_content,
                    placement_id: ad.placement_id.into(),
                    count: ad.count.unwrap_or(1),
                }),
                MozAdType::Tile => tile_ad_requests.push(MozAdsPlacementRequest {
                    iab_content: ad.iab_content,
                    placement_id: ad.placement_id.into(),
                }),
            }
        }

        // Dispatch image requests
        if !image_ad_requests.is_empty() {
            self.worker.dispatch(DispatchCommand::RequestImageAds {
                image_ad_requests,
                ohttp,
                cache_policy,
                flags: flags.clone(),
                blocks: blocks.clone(),
            })?;
        }
        // Dispatch spoc requests
        if !spoc_ad_requests.is_empty() {
            self.worker.dispatch(DispatchCommand::RequestSpocAds {
                spoc_ad_requests,
                ohttp,
                cache_policy,
                flags: flags.clone(),
                blocks: blocks.clone(),
            })?;
        }

        // Dispatch tiles requests
        if !tile_ad_requests.is_empty() {
            self.worker.dispatch(DispatchCommand::RequestTileAds {
                tile_ad_requests,
                ohttp,
                cache_policy,
                flags: flags.clone(),
                blocks: blocks.clone(),
            })?;
        }

        Ok(())
    }

    #[uniffi::method()]
    pub fn query_image_ads(&self, placement_id: PlacementId) -> Option<MozAdsImage> {
        use crate::mars::ad_response::AdImage;
        let inner = self.inner.lock();
        let image_ad: AdImage = inner.get_stored_ad_images(&placement_id)?;
        Some(image_ad.into())
    }

    #[uniffi::method()]
    pub fn query_spoc_ads(&self, placement_id: PlacementId) -> Option<Vec<MozAdsSpoc>> {
        use crate::mars::ad_response::AdSpoc;
        let inner = self.inner.lock();
        let spoc_ads: Vec<AdSpoc> = inner.get_stored_ad_spocs(&placement_id)?;
        Some(spoc_ads.into_iter().map(|ad| ad.into()).collect())
    }

    #[uniffi::method()]
    pub fn query_tile_ads(&self, placement_id: PlacementId) -> Option<MozAdsTile> {
        use crate::mars::ad_response::AdTile;
        let inner = self.inner.lock();
        let tile_ad: AdTile = inner.get_stored_ad_tile(&placement_id)?;
        Some(tile_ad.into())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_click(
        &self,
        click_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&click_url)
            .map_err(|e| ComponentError::RecordClick(CallbackRequestError::InvalidUrl(e).into()))?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);

        // After stateful suite is available, we allow usage of old blocking record/report functions if worker is not provided/available.
        // Prefetch functions instead return an error rather than default, because there is no default option for them.
        if self.worker.check_available() {
            self.worker
                .dispatch(DispatchCommand::RecordClick { url, ohttp })
        } else {
            let inner = self.inner.lock();
            inner
                .record_click(url, ohttp)
                .map_err(ComponentError::RecordClick)
        }
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_impression(
        &self,
        impression_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&impression_url).map_err(|e| {
            ComponentError::RecordImpression(CallbackRequestError::InvalidUrl(e).into())
        })?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);

        // After stateful suite is available, we allow usage of old blocking record/report functions if worker is not provided/available.
        // Prefetch functions instead return an error rather than default, because there is no default option for them.
        if self.worker.check_available() {
            self.worker
                .dispatch(DispatchCommand::RecordImpression { url, ohttp })
        } else {
            let inner = self.inner.lock();
            inner
                .record_impression(url, ohttp)
                .map_err(ComponentError::RecordImpression)
        }
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn report_ad(
        &self,
        report_url: String,
        reason: MozAdsReportReason,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&report_url)
            .map_err(|e| ComponentError::ReportAd(CallbackRequestError::InvalidUrl(e).into()))?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);

        // After stateful suite is available, we allow usage of old blocking record/report functions if worker is not provided/available.
        // Prefetch functions instead return an error rather than default, because there is no default option for them.
        if self.worker.check_available() {
            self.worker.dispatch(DispatchCommand::ReportAd {
                url,
                reason: reason.into(),
                ohttp,
            })
        } else {
            let inner = self.inner.lock();
            inner
                .report_ad(url, reason.into(), ohttp)
                .map_err(ComponentError::ReportAd)
        }
    }

    // Pings the background worker and waits for a response back, for use in tests.
    // Because the background worker is synchronous, this returns if the worker is empty,
    // making it useful for integration tests to wait until all tasks have completed.
    #[handle_error(ComponentError)]
    pub fn ping_background_worker(&self, timeout: Option<Duration>) -> AdsClientApiResult<()> {
        let (tx, rx) = mpsc::sync_channel(0);
        self.worker.dispatch(DispatchCommand::Ping(tx))?;

        if let Some(timeout) = timeout {
            rx.recv_timeout(timeout)
                .map_err(BackgroundWorkerError::from)?;
        } else {
            rx.recv().map_err(|_| BackgroundWorkerError::Closed)?;
        }
        Ok(())
    }
}
