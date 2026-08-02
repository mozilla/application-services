/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{collections::HashMap, sync::{Arc, mpsc::SyncSender}, thread::JoinHandle};

use client::error::ComponentError;
use error_support::handle_error;
use mars::error::CallbackRequestError;
use parking_lot::Mutex;
use url::Url as AdsClientUrl;

use client::AdsClient;
use http_cache::CachePolicy;
use mars::ad_request::{AdPlacementRequest, AdRequestFlags};

mod client;
mod ffi;
pub mod http_cache;
mod mars;
pub mod telemetry;
pub mod worker;
pub mod ads_cache;

pub use ffi::*;

use crate::{client::error::RequestAdsError, ffi::telemetry::MozAdsTelemetryWrapper, mars::ad_response::AdImage, worker::{DispatchCommand, ErrorOnlyRequestCallback}};

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

uniffi::custom_type!(AdsClientUrl, String, {
    remote,
    try_lift: |val| Ok(AdsClientUrl::parse(&val)?),
    lower: |obj| obj.as_str().to_string(),
});

#[derive(uniffi::Object)]
pub struct MozAdsClient {
    inner: MozAdsClientInner,

    _worker_thread: Option<JoinHandle<()>>,
    worker_dispatch: Option<SyncSender<DispatchCommand>>,
}

pub type MozAdsClientInner = Arc<Mutex<AdsClient<MozAdsTelemetryWrapper>>>;

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
        let cache_policy: CachePolicy = options.into();
        let response = inner
            .request_image_ads(requests, flags, Some(cache_policy), ohttp)
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
        let cache_policy: CachePolicy = options.into();
        let response = inner
            .request_spoc_ads(requests, flags, Some(cache_policy), ohttp)
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
        let cache_policy: CachePolicy = options.into();
        let response = inner
            .request_tile_ads(requests, flags, Some(cache_policy), ohttp)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn prefetch_image_ads(
        &self,
        image_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
        callback: Option<Box<dyn ErrorOnlyRequestCallback>>
    ) -> AdsClientApiResult<()> {
        if let Some(worker_dispatch) = &self.worker_dispatch {
            worker_dispatch.try_send(DispatchCommand::RequestImageAds { image_ad_requests, options, callback }).map_err(RequestAdsError::from)?;
            Ok(())
        } else {
            Err(RequestAdsError::BackgroundWorkerClosedError.into())
        }
    }

    #[handle_error(ComponentError)]
    #[uniffi::method()]
    pub fn query_image_ads(
        &self,
        placement_id: String,
    ) -> AdsClientApiResult<Option<MozAdsImage>> {
        let inner = self.inner.lock();
        let image_ads : Option<&Vec<AdImage>> = inner.get_cached_ads(&placement_id);
        Ok(image_ads.and_then(|ad| ad.into_iter().next().cloned()).map(|ad| ad.into()))
    }
}
