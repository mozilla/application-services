/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

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

pub use ffi::*;

use crate::{ffi::telemetry::MozAdsTelemetryWrapper, telemetry::Telemetry};

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
    inner: Mutex<AdsClient<MozAdsTelemetryWrapper>>,
    shutdown_references: ShutdownReferences,
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
    // Currently it is not possible to return an error, but it may yet be possible to do so, so we keep the Result.
    #[uniffi::method()]
    pub fn shutdown(&self) -> AdsClientApiResult<()> {
        self.shutdown_references.shutdown();
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

pub struct ShutdownReferences {
    telemetry: MozAdsTelemetryWrapper,
}

impl ShutdownReferences {
    fn shutdown(&self) {
        self.telemetry.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use crate::MozAdsClientBuilder;
    use std::{sync::mpsc, thread, time::Duration};

    fn test_timeout<F>(timeout: Duration, func: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            func();
            tx.send(())
                .expect("Internal test error: Could not send completion signal");
        });

        match rx.recv_timeout(timeout) {
            Ok(_) => handle.join().unwrap(),
            Err(_) => panic!("Test exceeded timeout duration"),
        }
    }
    #[test]
    fn shutdown_does_not_require_ads_client_lock() {
        test_timeout(Duration::from_secs(5), || {
            let builder = MozAdsClientBuilder::new().build();
            let lock = builder.inner.lock();

            // Holding a inner lock, we try to run shutdown.
            builder.shutdown().unwrap();

            // We explicitly drop the lock at the end.
            drop(lock);
        });
    }
}
