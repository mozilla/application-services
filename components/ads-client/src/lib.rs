/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{
    collections::HashMap,
    sync::{
        mpsc::SyncSender,
        Arc,
    },
    thread::JoinHandle,
};

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

pub use ffi::*;

use crate::{
    ffi::telemetry::MozAdsTelemetryWrapper,
    worker::{
        DispatchCommand, ErrorOnlyRequestCallback, ImageRequestCallback, SpocRequestCallback,
        TileRequestCallback,
    },
};
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
    inner: Arc<Mutex<AdsClient<MozAdsTelemetryWrapper>>>,
    _worker_thread_handle: JoinHandle<()>,
    command_tx: SyncSender<DispatchCommand>,
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

    // TODO: Remove the following functions when uniffi = 0.32 lands, and we can properly expose the available enum directly with `dispatch(command)`.
    #[handle_error(ComponentError)]
    #[uniffi::method()]
    pub fn dispatch_record_click(
        &self,
        click_url: String,
        options: Option<MozAdsCallbackOptions>,
        callback: Box<dyn ErrorOnlyRequestCallback>,
    ) -> AdsClientApiResult<()> {
        self.dispatch(DispatchCommand::RecordClick {
            click_url,
            options,
            callback,
        })
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_record_impression(
        &self,
        impression_url: String,
        options: Option<MozAdsCallbackOptions>,
        callback: Box<dyn ErrorOnlyRequestCallback>,
    ) -> AdsClientApiResult<()> {
        self.dispatch(DispatchCommand::RecordImpression {
            impression_url,
            options,
            callback,
        })
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_report_ad(
        &self,
        report_url: String,
        reason: MozAdsReportReason,
        options: Option<MozAdsCallbackOptions>,
        callback: Box<dyn ErrorOnlyRequestCallback>,
    ) -> AdsClientApiResult<()> {
        self.dispatch(DispatchCommand::ReportAd {
            report_url,
            reason,
            options,
            callback,
        })
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_request_image_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
        callback: Box<dyn ImageRequestCallback>,
    ) -> AdsClientApiResult<()> {
        self.dispatch(DispatchCommand::RequestImageAds {
            moz_ad_requests,
            options,
            callback,
        })
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_request_spoc_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
        callback: Box<dyn SpocRequestCallback>,
    ) -> AdsClientApiResult<()> {
        self.dispatch(DispatchCommand::RequestSpocAds {
            moz_ad_requests,
            options,
            callback,
        })
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_request_tile_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
        callback: Box<dyn TileRequestCallback>,
    ) -> AdsClientApiResult<()> {
        self.dispatch(DispatchCommand::RequestTileAd {
            moz_ad_requests,
            options,
            callback,
        })
    }
}

impl MozAdsClient {
    // TODO: The following
    pub fn dispatch(&self, command: DispatchCommand) -> Result<(), ComponentError> {
        // try_send provides an error on a disconnection or a SyncChannel full buffer
        // TODO: drop oldest on error, not newest
        self.command_tx
            .try_send(command)
            .map_err(ComponentError::Dispatch)?;
        Ok(())
    }
}
