/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use error::AdsClientApiResult;
use error::ComponentError;
use error_support::handle_error;
use parking_lot::Mutex;
use url::Url as AdsClientUrl;

use client::MozAdsClientInner;
use error::AdsClientApiError;
use http_cache::{CacheMode, RequestCachePolicy};
use instrument::TrackError;

use crate::client::ad_request::AdContentCategory;
use crate::client::ad_request::AdPlacementRequest;
use crate::client::ad_request::IABContentTaxonomy;
use crate::client::ad_response::MozAd;
use crate::client::config::MozAdsClientConfig;

mod client;
mod error;
pub mod http_cache;
mod instrument;
mod mars;

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

uniffi::custom_type!(AdsClientUrl, String, {
    remote,
    try_lift: |val| Ok(AdsClientUrl::parse(&val)?),
    lower: |obj| obj.as_str().to_string(),
});

/// Top-level API for the mac component
#[derive(uniffi::Object)]
pub struct MozAdsClient {
    inner: Mutex<MozAdsClientInner>,
}

#[uniffi::export]
impl MozAdsClient {
    #[uniffi::constructor]
    pub fn new(client_config: Option<MozAdsClientConfig>) -> Self {
        Self {
            inner: Mutex::new(MozAdsClientInner::new(client_config)),
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
        let placements = inner
            .request_ads(requests, options)
            .map_err(ComponentError::RequestAds)?;
        let placements = placements
            .into_iter()
            .filter_map(|(placement_id, mut vec)| {
                vec.pop().map(|placement| (placement_id, placement))
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
        let placements = inner
            .request_ads(requests, options)
            .map_err(ComponentError::RequestAds)?;
        Ok(placements)
    }

    #[handle_error(ComponentError)]
    pub fn record_impression(&self, placement: MozAd) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .record_impression(&placement)
            .map_err(ComponentError::RecordImpression)
            .emit_telemetry_if_error()
    }

    #[handle_error(ComponentError)]
    pub fn record_click(&self, placement: MozAd) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .record_click(&placement)
            .map_err(ComponentError::RecordClick)
            .emit_telemetry_if_error()
    }

    #[handle_error(ComponentError)]
    pub fn report_ad(&self, placement: MozAd) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .report_ad(&placement)
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
        inner.clear_cache().map_err(|_| AdsClientApiError::Other {
            reason: "Failed to clear cache".to_string(),
        })
    }
}

#[derive(uniffi::Record)]
pub struct MozAdsRequestOptions {
    pub cache_policy: Option<RequestCachePolicy>,
}

impl Default for MozAdsRequestOptions {
    fn default() -> Self {
        Self {
            cache_policy: Some(RequestCachePolicy {
                mode: CacheMode::default(),
                ttl_seconds: None,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct IABContent {
    pub taxonomy: IABContentTaxonomy,
    pub category_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequest {
    pub placement_id: String,
    pub iab_content: Option<IABContent>,
}

impl From<&MozAdsPlacementRequest> for AdPlacementRequest {
    fn from(request: &MozAdsPlacementRequest) -> Self {
        AdPlacementRequest {
            placement: request.placement_id.clone(),
            count: 1,
            content: request
                .iab_content
                .as_ref()
                .map(|iab_content| AdContentCategory {
                    taxonomy: iab_content.taxonomy,
                    categories: iab_content.category_ids.clone(),
                }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequestWithCount {
    pub count: u32,
    pub placement_id: String,
    pub iab_content: Option<IABContent>,
}

impl From<&MozAdsPlacementRequestWithCount> for AdPlacementRequest {
    fn from(request: &MozAdsPlacementRequestWithCount) -> Self {
        AdPlacementRequest {
            placement: request.placement_id.clone(),
            count: request.count,
            content: request
                .iab_content
                .as_ref()
                .map(|iab_content| AdContentCategory {
                    taxonomy: iab_content.taxonomy,
                    categories: iab_content.category_ids.clone(),
                }),
        }
    }
}
