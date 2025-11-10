/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::client::ad_request::{AdContentCategory, AdPlacementRequest, IABContentTaxonomy};
use crate::client::ad_response::{Ad, AdCallbacks};
use crate::client::config::{AdsCacheConfig, AdsClientConfig, Environment};
use crate::error::ComponentError;
use crate::http_cache::{CacheMode, RequestCachePolicy};
use error_support::{ErrorHandling, GetErrorHandling};
use url::Url;

pub type AdsClientApiResult<T> = std::result::Result<T, MozAdsClientApiError>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MozAdsClientApiError {
    #[error("Something unexpected occurred.")]
    Other { reason: String },
}

impl From<context_id::ApiError> for MozAdsClientApiError {
    fn from(err: context_id::ApiError) -> Self {
        MozAdsClientApiError::Other {
            reason: err.to_string(),
        }
    }
}

impl GetErrorHandling for ComponentError {
    type ExternalError = MozAdsClientApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(MozAdsClientApiError::Other {
            reason: self.to_string(),
        })
    }
}

#[derive(uniffi::Record)]
pub struct MozAdsRequestOptions {
    pub cache_policy: Option<MozRequestCachePolicy>,
}

impl Default for MozAdsRequestOptions {
    fn default() -> Self {
        Self {
            cache_policy: Some(MozRequestCachePolicy {
                mode: MozCacheMode::default(),
                ttl_seconds: None,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozIABContent {
    pub taxonomy: MozIABContentTaxonomy,
    pub category_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequest {
    pub placement_id: String,
    pub iab_content: Option<MozIABContent>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequestWithCount {
    pub count: u32,
    pub placement_id: String,
    pub iab_content: Option<MozIABContent>,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}

#[derive(Default, uniffi::Record)]
pub struct MozAdsClientConfig {
    pub environment: MozEnvironment,
    pub cache_config: Option<MozAdsCacheConfig>,
}

#[derive(Clone, Copy, Debug, Default, uniffi::Enum, Eq, PartialEq)]
pub enum MozEnvironment {
    #[default]
    Prod,
    #[cfg(feature = "dev")]
    Staging,
}

#[derive(uniffi::Record)]
pub struct MozAdsCacheConfig {
    pub db_path: String,
    pub default_cache_ttl_seconds: Option<u64>,
    pub max_size_mib: Option<u64>,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdContentCategory {
    pub taxonomy: MozIABContentTaxonomy,
    pub categories: Vec<String>,
}

#[derive(Clone, Copy, Debug, uniffi::Enum, PartialEq)]
pub enum MozIABContentTaxonomy {
    IAB1_0,
    IAB2_0,
    IAB2_1,
    IAB2_2,
    IAB3_0,
}

#[derive(Clone, Copy, Debug, Default, uniffi::Record)]
pub struct MozRequestCachePolicy {
    pub mode: MozCacheMode,
    pub ttl_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, uniffi::Enum)]
pub enum MozCacheMode {
    #[default]
    CacheFirst,
    NetworkFirst,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAd {
    pub alt_text: Option<String>,
    pub block_key: String,
    pub callbacks: MozAdCallbacks,
    pub format: String,
    pub image_url: String,
    pub url: String,
}

impl From<AdCallbacks> for MozAdCallbacks {
    fn from(callbacks: AdCallbacks) -> Self {
        Self {
            click: callbacks.click,
            impression: callbacks.impression,
            report: callbacks.report,
        }
    }
}

impl From<MozAdCallbacks> for AdCallbacks {
    fn from(callbacks: MozAdCallbacks) -> Self {
        Self {
            click: callbacks.click,
            impression: callbacks.impression,
            report: callbacks.report,
        }
    }
}

impl From<Environment> for MozEnvironment {
    fn from(env: Environment) -> Self {
        match env {
            Environment::Prod => MozEnvironment::Prod,
            #[cfg(feature = "dev")]
            Environment::Staging => MozEnvironment::Staging,
        }
    }
}

impl From<MozEnvironment> for Environment {
    fn from(env: MozEnvironment) -> Self {
        match env {
            MozEnvironment::Prod => Environment::Prod,
            #[cfg(feature = "dev")]
            MozEnvironment::Staging => Environment::Staging,
        }
    }
}

impl From<IABContentTaxonomy> for MozIABContentTaxonomy {
    fn from(taxonomy: IABContentTaxonomy) -> Self {
        match taxonomy {
            IABContentTaxonomy::IAB1_0 => MozIABContentTaxonomy::IAB1_0,
            IABContentTaxonomy::IAB2_0 => MozIABContentTaxonomy::IAB2_0,
            IABContentTaxonomy::IAB2_1 => MozIABContentTaxonomy::IAB2_1,
            IABContentTaxonomy::IAB2_2 => MozIABContentTaxonomy::IAB2_2,
            IABContentTaxonomy::IAB3_0 => MozIABContentTaxonomy::IAB3_0,
        }
    }
}

impl From<MozIABContentTaxonomy> for IABContentTaxonomy {
    fn from(taxonomy: MozIABContentTaxonomy) -> Self {
        match taxonomy {
            MozIABContentTaxonomy::IAB1_0 => IABContentTaxonomy::IAB1_0,
            MozIABContentTaxonomy::IAB2_0 => IABContentTaxonomy::IAB2_0,
            MozIABContentTaxonomy::IAB2_1 => IABContentTaxonomy::IAB2_1,
            MozIABContentTaxonomy::IAB2_2 => IABContentTaxonomy::IAB2_2,
            MozIABContentTaxonomy::IAB3_0 => IABContentTaxonomy::IAB3_0,
        }
    }
}

impl From<MozRequestCachePolicy> for RequestCachePolicy {
    fn from(policy: MozRequestCachePolicy) -> Self {
        Self {
            mode: policy.mode.into(),
            ttl_seconds: policy.ttl_seconds,
        }
    }
}

impl From<CacheMode> for MozCacheMode {
    fn from(mode: CacheMode) -> Self {
        match mode {
            CacheMode::CacheFirst => MozCacheMode::CacheFirst,
            CacheMode::NetworkFirst => MozCacheMode::NetworkFirst,
        }
    }
}

impl From<MozCacheMode> for CacheMode {
    fn from(mode: MozCacheMode) -> Self {
        match mode {
            MozCacheMode::CacheFirst => CacheMode::CacheFirst,
            MozCacheMode::NetworkFirst => CacheMode::NetworkFirst,
        }
    }
}

impl From<Ad> for MozAd {
    fn from(ad: Ad) -> Self {
        Self {
            alt_text: ad.alt_text,
            block_key: ad.block_key,
            callbacks: ad.callbacks.into(),
            format: ad.format,
            image_url: ad.image_url,
            url: ad.url,
        }
    }
}

impl From<MozAd> for Ad {
    fn from(ad: MozAd) -> Self {
        Self {
            alt_text: ad.alt_text,
            block_key: ad.block_key,
            callbacks: ad.callbacks.into(),
            format: ad.format,
            image_url: ad.image_url,
            url: ad.url,
        }
    }
}

impl From<&MozIABContent> for AdContentCategory {
    fn from(content: &MozIABContent) -> Self {
        Self {
            taxonomy: content.taxonomy.into(),
            categories: content.category_ids.clone(),
        }
    }
}

impl From<MozAdsRequestOptions> for RequestCachePolicy {
    fn from(options: MozAdsRequestOptions) -> Self {
        options.cache_policy.map(Into::into).unwrap_or_default()
    }
}

impl From<Option<MozAdsRequestOptions>> for RequestCachePolicy {
    fn from(options: Option<MozAdsRequestOptions>) -> Self {
        options.map(Into::into).unwrap_or_default()
    }
}

impl From<MozAdsClientConfig> for AdsClientConfig {
    fn from(config: MozAdsClientConfig) -> Self {
        Self {
            environment: config.environment.into(),
            cache_config: config.cache_config.map(Into::into),
        }
    }
}

impl From<MozAdsCacheConfig> for AdsCacheConfig {
    fn from(config: MozAdsCacheConfig) -> Self {
        Self {
            db_path: config.db_path,
            default_cache_ttl_seconds: config.default_cache_ttl_seconds,
            max_size_mib: config.max_size_mib,
        }
    }
}

impl From<&MozAdsPlacementRequest> for AdPlacementRequest {
    fn from(request: &MozAdsPlacementRequest) -> Self {
        Self {
            placement: request.placement_id.clone(),
            count: 1,
            content: request.iab_content.as_ref().map(Into::into),
        }
    }
}

impl From<&MozAdsPlacementRequestWithCount> for AdPlacementRequest {
    fn from(request: &MozAdsPlacementRequestWithCount) -> Self {
        Self {
            placement: request.placement_id.clone(),
            count: request.count,
            content: request.iab_content.as_ref().map(Into::into),
        }
    }
}
