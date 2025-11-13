/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use error::ComponentError;
use error_support::handle_error;
use parking_lot::Mutex;
use url::Url as AdsClientUrl;

use client::ad_request::AdPlacementRequest;
use client::ad_response::Ad;
use client::AdsClient;
use http_cache::RequestCachePolicy;
use instrument::TrackError;

mod client;
mod error;
mod ffi;
pub mod http_cache;
mod instrument;
mod mars;

pub use ffi::*;

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
    inner: Mutex<AdsClient>,
}

#[uniffi::export]
impl MozAdsClient {
    #[uniffi::constructor]
    pub fn new(client_config: Option<MozAdsClientConfig>) -> Self {
        let config = client_config.map(Into::into);
        Self {
            inner: Mutex::new(AdsClient::new(config)),
        }
    }

    #[handle_error(ComponentError)]
    pub fn request_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAd>> {
        let inner = self.inner.lock();

        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let cache_policy: RequestCachePolicy = options.into();
        let placements = inner
            .request_ads(requests, Some(cache_policy))
            .map_err(ComponentError::RequestAds)?;
        let placements = placements
            .into_iter()
            .filter_map(|(placement_id, mut vec)| {
                vec.pop().map(|placement| (placement_id, placement.into()))
            })
            .collect();
        Ok(placements)
    }

    #[handle_error(ComponentError)]
    pub fn request_ads_multiset(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAd>>> {
        let inner = self.inner.lock();

        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let cache_policy: RequestCachePolicy = options.into();
        let placements = inner
            .request_ads(requests, Some(cache_policy))
            .map_err(ComponentError::RequestAds)?;
        let placements = placements
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(Into::into).collect()))
            .collect();
        Ok(placements)
    }

    #[handle_error(ComponentError)]
    pub fn record_impression(&self, placement: MozAd) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        let ad: Ad = placement.into();
        inner
            .record_impression(&ad)
            .map_err(ComponentError::RecordImpression)
            .emit_telemetry_if_error()
    }

    #[handle_error(ComponentError)]
    pub fn record_click(&self, placement: MozAd) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        let ad: Ad = placement.into();
        inner
            .record_click(&ad)
            .map_err(ComponentError::RecordClick)
            .emit_telemetry_if_error()
    }

    #[handle_error(ComponentError)]
    pub fn report_ad(&self, placement: MozAd) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        let ad: Ad = placement.into();
        inner
            .report_ad(&ad)
            .map_err(ComponentError::ReportAd)
            .emit_telemetry_if_error()
    }

    pub fn cycle_context_id(&self) -> AdsClientApiResult<String> {
        let mut inner = self.inner.lock();
        let previous_context_id = inner.cycle_context_id()?;
        Ok(previous_context_id)
    }

    pub fn clear_cache(&self) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .clear_cache()
            .map_err(|_| MozAdsClientApiError::Other {
                reason: "Failed to clear cache".to_string(),
            })
    }
}
