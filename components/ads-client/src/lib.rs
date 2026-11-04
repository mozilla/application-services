/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{collections::HashMap, sync::Arc};

use client::error::ComponentError;
use error_support::handle_error;
use mars::error::CallbackRequestError;
pub use std::{collections::HashMap, sync::Arc};
#[cfg(feature = "stateful")]
use std::{sync::mpsc, time::Duration};
use url::Url as AdsClientUrl;

use client::AdsClient;
use http_cache::CachePolicy;
use mars::ad_request::{AdPlacementRequest, AdRequestFlags};
mod ads;
#[cfg(feature = "stateful")]
pub mod ads_store;
pub mod bytesize;
mod client;
pub mod clock;
mod ffi;
pub mod http_cache;
mod mars;
#[cfg(feature = "stateful")]
pub mod request;
pub mod telemetry;
#[cfg(feature = "stateful")]
pub mod worker;

pub use ffi::*;

/// Path that makes SQLite open a private in-memory database.
pub(crate) const IN_MEMORY_DB_PATH: &str = ":memory:";

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

uniffi::custom_type!(AdsClientUrl, String, {
    remote,
    try_lift: |val| Ok(AdsClientUrl::parse(&val)?),
    lower: |obj| obj.as_str().to_owned(),
});
#[cfg(feature = "stateful")]
uniffi::custom_type!(PlacementId, String);

/// Requests ads from MARS and records their clicks, impressions and reports.
/// Build it with [`MozAdsClientBuilder`], ideally once per app.
#[derive(uniffi::Object)]
pub struct MozAdsClient {
    inner: Arc<AdsClient>,
    shutdown_references: ShutdownReferences<MozAdsTelemetryWrapper>,
    #[cfg(feature = "stateful")]
    worker: BackgroundWorker,
}

#[uniffi::export]
impl MozAdsClient {
    /// Deletes everything in the HTTP cache.
    #[handle_error(ComponentError)]
    pub fn clear_cache(&self) -> AdsClientApiResult<()> {
        self.inner.clear_cache().map_err(ComponentError::ClearCache)
    }

    /// Records a click on an ad, from its `callbacks.click` URL.
    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_click(
        &self,
        click_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&click_url)
            .map_err(|e| ComponentError::RecordClick(CallbackRequestError::InvalidUrl(e).into()))?;
        let ohttp = options.is_some_and(|o| o.ohttp);
        self.inner
            .record_click(url, ohttp)
            .map_err(ComponentError::RecordClick)
    }

    /// Records an impression of an ad, from its `callbacks.impression` URL.
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
        let ohttp = options.is_some_and(|o| o.ohttp);
        self.inner
            .record_impression(url, ohttp)
            .map_err(ComponentError::RecordImpression)
    }

    /// Reports an ad, from its `callbacks.report` URL.
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
        let ohttp = options.is_some_and(|o| o.ohttp);
        self.inner
            .report_ad(url, reason.into(), ohttp)
            .map_err(ComponentError::ReportAd)
    }

    /// Requests one image ad per placement. The result is keyed by
    /// `placement_id` and leaves out placements with no ad.
    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_image_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsImage>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(Into::into).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = self
            .inner
            .request_image_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    /// Requests `count` spocs per placement. The result is keyed by
    /// `placement_id` and leaves out placements with no ad.
    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_spoc_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAdsSpoc>>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(Into::into).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = self
            .inner
            .request_spoc_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds)?;
        Ok(response
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(Into::into).collect()))
            .collect())
    }

    /// Requests `count` ads per placement, for placements that can return
    /// either spocs or image ads. The result is keyed by `placement_id` and
    /// leaves out placements with no ad.
    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_spoc_or_image_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAdsImageOrSpoc>>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(Into::into).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = self
            .inner
            .request_spoc_or_image_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds)?;
        Ok(response
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(Into::into).collect()))
            .collect())
    }

    /// Requests one tile ad per placement. The result is keyed by
    /// `placement_id` and leaves out placements with no ad.
    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_tile_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsTile>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(Into::into).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let relevance = options.relevance;
        let response = self
            .inner
            .request_tile_ads(requests, flags, cache_policy, ohttp, blocks, relevance)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    /// Allows the ads-client to unload some references and prepare for a safe shutdown.
    /// Other methods should not be called after this one.
    /// Currently, we shutdown (with no expected errors).
    /// However, future shutdowns may require an error to be logged, so we keep the Result.
    #[uniffi::method()]
    pub fn shutdown(&self) -> AdsClientApiResult<()> {
        self.inner.shutdown();
        Ok(())
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
