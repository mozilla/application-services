/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, SyncSender},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use client::error::ComponentError;
use error_support::handle_error;
use mars::error::CallbackRequestError;
use parking_lot::Mutex;
use url::Url as AdsClientUrl;

use client::AdsClient;
use error_support::error;
use http_cache::CachePolicy;
use mars::ad_request::{AdPlacementRequest, AdRequestFlags};
pub mod ads_cache;
mod client;
mod ffi;
pub mod http_cache;
mod mars;
pub mod telemetry;
pub mod worker;

pub use ffi::*;

use crate::{
    client::error::BackgroundWorkerError,
    ffi::telemetry::MozAdsTelemetryWrapper,
    mars::ad_response::{AdImage, AdSpoc, AdTile},
    worker::{Dispatch, DispatchCommand},
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
    inner: MozAdsClientInner,

    _worker_thread: Option<JoinHandle<()>>,
    worker_dispatch: Option<SyncSender<Dispatch>>,
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

    // Allows the ads-client to unload some references and prepare for a safe shutdown.
    // Other methods should not be called after this one.
    #[uniffi::method()]
    pub fn shutdown(&self) -> AdsClientApiResult<()> {
        let mut inner = self.inner.lock();
        if let Err(err) = inner.shutdown_client() {
            // Log the error, but continue with shutdown.
            error!("Failed to shutdown the ads client: {:?}", err);
        }
        Ok(())
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
    #[uniffi::method(default(image_ad_requests = [], spoc_ad_requests = [], tile_ad_requests = [], options = None))]
    pub fn prefetch_ads(
        &self,
        image_ad_requests: Vec<MozAdsPlacementRequest>,
        spoc_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        tile_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<()> {
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy: CachePolicy = options.into();

        if let Some(worker_dispatch) = &self.worker_dispatch {
            // Dispatch image requests
            if !image_ad_requests.is_empty() {
                worker_dispatch
                    .try_send(Dispatch {
                        command: DispatchCommand::RequestImageAds {
                            image_ad_requests,
                            ohttp,
                            cache_policy,
                            flags: flags.clone(),
                        },
                    })
                    .map_err(BackgroundWorkerError::from)?;
            }

            // Dispatch spoc requests
            if !spoc_ad_requests.is_empty() {
                worker_dispatch
                    .try_send(Dispatch {
                        command: DispatchCommand::RequestSpocAds {
                            spoc_ad_requests,
                            ohttp,
                            cache_policy,
                            flags: flags.clone(),
                        },
                    })
                    .map_err(BackgroundWorkerError::from)?;
            }

            // Dispatch tiles requests
            if !tile_ad_requests.is_empty() {
                worker_dispatch
                    .try_send(Dispatch {
                        command: DispatchCommand::RequestTileAds {
                            tile_ad_requests,
                            ohttp,
                            cache_policy,
                            flags: flags.clone(),
                        },
                    })
                    .map_err(BackgroundWorkerError::from)?;
            }
            Ok(())
        } else {
            Err(BackgroundWorkerError::WorkerClosed.into())
        }
    }

    #[handle_error(ComponentError)]
    #[uniffi::method()]
    pub fn query_image_ads(&self, placement_id: String) -> AdsClientApiResult<Option<MozAdsImage>> {
        let inner = self.inner.lock();
        let image_ads: Option<&AdImage> = inner.get_cached_ads::<AdImage>(&placement_id);
        Ok(image_ads.map(|ad| ad.clone().into()))
    }

    #[handle_error(ComponentError)]
    #[uniffi::method()]
    pub fn query_spoc_ads(
        &self,
        placement_id: String,
    ) -> AdsClientApiResult<Option<Vec<MozAdsSpoc>>> {
        let inner = self.inner.lock();
        let spoc_ads: Option<&Vec<AdSpoc>> = inner.get_cached_ads::<AdSpoc>(&placement_id);
        Ok(spoc_ads.map(|res| res.iter().map(|ad| ad.clone().into()).collect()))
    }

    #[handle_error(ComponentError)]
    #[uniffi::method()]
    pub fn query_tile_ads(&self, placement_id: String) -> AdsClientApiResult<Option<MozAdsTile>> {
        let inner = self.inner.lock();
        let image_ads: Option<&AdTile> = inner.get_cached_ads::<AdTile>(&placement_id);
        Ok(image_ads.map(|ad| ad.clone().into()))
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_record_click(
        &self,
        click_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        if let Some(worker_dispatch) = &self.worker_dispatch {
            let url = AdsClientUrl::parse(&click_url).map_err(|e| {
                ComponentError::RecordClick(CallbackRequestError::InvalidUrl(e).into())
            })?;
            let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
            worker_dispatch
                .try_send(Dispatch {
                    command: DispatchCommand::RecordClick { url, ohttp },
                })
                .map_err(BackgroundWorkerError::from)?;
            Ok(())
        } else {
            Err(BackgroundWorkerError::WorkerClosed.into())
        }
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_record_impression(
        &self,
        impression_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        if let Some(worker_dispatch) = &self.worker_dispatch {
            let url = AdsClientUrl::parse(&impression_url).map_err(|e| {
                ComponentError::RecordImpression(CallbackRequestError::InvalidUrl(e).into())
            })?;
            let ohttp = options.map(|o| o.ohttp).unwrap_or(false);

            worker_dispatch
                .try_send(Dispatch {
                    command: DispatchCommand::RecordImpression { url, ohttp },
                })
                .map_err(BackgroundWorkerError::from)?;

            Ok(())
        } else {
            Err(BackgroundWorkerError::WorkerClosed.into())
        }
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn dispatch_report_ad(
        &self,
        report_url: String,
        reason: MozAdsReportReason,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        if let Some(worker_dispatch) = &self.worker_dispatch {
            let url = AdsClientUrl::parse(&report_url).map_err(|e| {
                ComponentError::ReportAd(CallbackRequestError::InvalidUrl(e).into())
            })?;
            let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
            worker_dispatch
                .try_send(Dispatch {
                    command: DispatchCommand::ReportAd {
                        url,
                        reason: reason.into(),
                        ohttp,
                    },
                })
                .map_err(BackgroundWorkerError::from)?;

            Ok(())
        } else {
            Err(BackgroundWorkerError::WorkerClosed.into())
        }
    }

    // Pings the background worker and waits for a response back, for use in tests.
    // Because the background worker is synchronous, this returns if the worker is empty,
    // making it useful for integration tests to wait until all tasks have completed.
    #[handle_error(ComponentError)]
    pub fn ping_background_worker(&self, timeout: Option<Duration>) -> AdsClientApiResult<()> {
        if let Some(worker_dispatch) = &self.worker_dispatch {
            let (tx, rx) = mpsc::sync_channel(0);
            worker_dispatch
                .try_send(Dispatch {
                    command: DispatchCommand::Ping(tx),
                })
                .map_err(BackgroundWorkerError::from)?;
            if let Some(timeout) = timeout {
                rx.recv_timeout(timeout)
                    .map_err(BackgroundWorkerError::from)?;
            } else {
                rx.recv().map_err(|_| BackgroundWorkerError::WorkerClosed)?;
            }
            return Ok(());
        } else {
            Err(BackgroundWorkerError::WorkerClosed.into())
        }
    }
}
