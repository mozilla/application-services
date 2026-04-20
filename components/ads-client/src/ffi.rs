/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod error;
pub mod telemetry;

use std::sync::Arc;

use crate::client::config::{AdsCacheConfig, AdsClientConfig};
use crate::client::{AdsClient, ContextIdProvider};
use crate::ffi::telemetry::MozAdsTelemetryWrapper;
use crate::http_cache::CachePolicy;
use crate::mars::ad_request::{AdContentCategory, AdPlacementRequest, IABContentTaxonomy};
use crate::mars::ad_response::{
    AdCallbacks, AdImage, AdSpoc, AdTile, SpocFrequencyCaps, SpocRanking,
};
use crate::mars::Environment;
use crate::mars::ReportReason;
use crate::MozAdsClient;
use parking_lot::Mutex;
use url::Url;

pub use error::{AdsClientApiResult, MozAdsClientApiError};
pub use telemetry::MozAdsTelemetry;

// TODO: Temporary workaround for HNT requirements — do not use for new integrations.
// Context ID management should remain internal to the ads client and this interface should be removed.
#[uniffi::export(with_foreign)]
pub trait MozAdsContextIdProvider: Send + Sync {
    fn context_id(&self) -> String;
}

struct MozAdsContextIdProviderWrapper(Arc<dyn MozAdsContextIdProvider>);

impl MozAdsContextIdProviderWrapper {
    fn new(provider: Arc<dyn MozAdsContextIdProvider>) -> Self {
        Self(provider)
    }
}

impl ContextIdProvider for MozAdsContextIdProviderWrapper {
    fn context_id(&self) -> context_id::ApiResult<String> {
        Ok(self.0.context_id())
    }
}

impl From<MozAdsContextIdProviderWrapper> for Box<dyn ContextIdProvider> {
    fn from(wrapper: MozAdsContextIdProviderWrapper) -> Self {
        Box::new(wrapper)
    }
}

#[derive(Default, uniffi::Record)]
pub struct MozAdsRequestOptions {
    pub cache_policy: Option<MozAdsCachePolicy>,
    #[uniffi(default = false)]
    pub ohttp: bool,
}

#[derive(Default, uniffi::Record)]
pub struct MozAdsCallbackOptions {
    #[uniffi(default = false)]
    pub ohttp: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsIABContent {
    pub category_ids: Vec<String>,
    pub taxonomy: MozAdsIABContentTaxonomy,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequest {
    #[uniffi(default = None)]
    pub iab_content: Option<MozAdsIABContent>,
    pub placement_id: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequestWithCount {
    pub count: u32,
    #[uniffi(default = None)]
    pub iab_content: Option<MozAdsIABContent>,
    pub placement_id: String,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}

#[derive(uniffi::Object)]
pub struct MozAdsClientBuilder(Mutex<MozAdsClientBuilderInner>);

#[derive(Default)]
struct MozAdsClientBuilderInner {
    cache_config: Option<MozAdsCacheConfig>,
    context_id_provider: Option<Arc<dyn MozAdsContextIdProvider>>,
    environment: Option<MozAdsEnvironment>,
    telemetry: Option<Arc<dyn MozAdsTelemetry>>,
}

impl Default for MozAdsClientBuilder {
    fn default() -> Self {
        Self(Mutex::new(MozAdsClientBuilderInner::default()))
    }
}

#[uniffi::export]
impl MozAdsClientBuilder {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(&self) -> MozAdsClient {
        let inner = self.0.lock();
        let client_config = AdsClientConfig {
            cache_config: inner.cache_config.clone().map(Into::into),
            context_id_provider: inner
                .context_id_provider
                .clone()
                .map(MozAdsContextIdProviderWrapper::new)
                .map(Into::into),
            environment: inner.environment.unwrap_or_default().into(),
            telemetry: inner
                .telemetry
                .clone()
                .map(MozAdsTelemetryWrapper::new)
                .unwrap_or_else(MozAdsTelemetryWrapper::noop),
        };
        let client = AdsClient::new(client_config);
        MozAdsClient {
            inner: Mutex::new(client),
        }
    }

    pub fn cache_config(self: Arc<Self>, cache_config: MozAdsCacheConfig) -> Arc<Self> {
        self.0.lock().cache_config = Some(cache_config);
        self
    }

    pub fn context_id_provider(
        self: Arc<Self>,
        provider: Arc<dyn MozAdsContextIdProvider>,
    ) -> Arc<Self> {
        self.0.lock().context_id_provider = Some(provider);
        self
    }

    pub fn environment(self: Arc<Self>, environment: MozAdsEnvironment) -> Arc<Self> {
        self.0.lock().environment = Some(environment);
        self
    }

    pub fn telemetry(self: Arc<Self>, telemetry: Arc<dyn MozAdsTelemetry>) -> Arc<Self> {
        self.0.lock().telemetry = Some(telemetry);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, uniffi::Enum, Eq, PartialEq)]
pub enum MozAdsEnvironment {
    #[default]
    Prod,
    Staging,
    #[cfg(test)]
    Test,
}

#[derive(Clone, uniffi::Record)]
pub struct MozAdsCacheConfig {
    pub db_path: String,
    #[uniffi(default = None)]
    pub default_cache_ttl_seconds: Option<u64>,
    #[uniffi(default = None)]
    pub max_size_mib: Option<u64>,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsContentCategory {
    pub categories: Vec<String>,
    pub taxonomy: MozAdsIABContentTaxonomy,
}

#[derive(Clone, Copy, Debug, uniffi::Enum, PartialEq)]
pub enum MozAdsIABContentTaxonomy {
    IAB1_0,
    IAB2_0,
    IAB2_1,
    IAB2_2,
    IAB3_0,
}

#[derive(Clone, Copy, Debug, Default, uniffi::Record)]
pub struct MozAdsCachePolicy {
    pub mode: MozAdsCacheMode,
    #[uniffi(default = None)]
    pub ttl_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum MozAdsReportReason {
    Inappropriate,
    NotInterested,
    SeenTooManyTimes,
}

impl From<MozAdsReportReason> for ReportReason {
    fn from(reason: MozAdsReportReason) -> Self {
        match reason {
            MozAdsReportReason::Inappropriate => ReportReason::Inappropriate,
            MozAdsReportReason::NotInterested => ReportReason::NotInterested,
            MozAdsReportReason::SeenTooManyTimes => ReportReason::SeenTooManyTimes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, uniffi::Enum)]
pub enum MozAdsCacheMode {
    #[default]
    CacheFirst,
    NetworkFirst,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsImage {
    pub alt_text: Option<String>,
    pub block_key: String,
    pub callbacks: MozAdsCallbacks,
    pub format: String,
    pub image_url: String,
    pub url: String,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsSpoc {
    pub block_key: String,
    pub callbacks: MozAdsCallbacks,
    pub caps: MozAdsSpocFrequencyCaps,
    pub domain: String,
    pub excerpt: String,
    pub format: String,
    pub image_url: String,
    pub ranking: MozAdsSpocRanking,
    pub sponsor: String,
    pub sponsored_by_override: Option<String>,
    pub title: String,
    pub url: String,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsSpocFrequencyCaps {
    pub cap_key: String,
    pub day: u32,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsSpocRanking {
    pub item_score: f64,
    pub personalization_models: std::collections::HashMap<String, u32>,
    pub priority: u32,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsTile {
    pub block_key: String,
    pub callbacks: MozAdsCallbacks,
    pub format: String,
    pub image_url: String,
    pub name: String,
    pub url: String,
}

impl From<AdCallbacks> for MozAdsCallbacks {
    fn from(callbacks: AdCallbacks) -> Self {
        Self {
            click: callbacks.click,
            impression: callbacks.impression,
            report: callbacks.report,
        }
    }
}

impl From<MozAdsCallbacks> for AdCallbacks {
    fn from(callbacks: MozAdsCallbacks) -> Self {
        Self {
            click: callbacks.click,
            impression: callbacks.impression,
            report: callbacks.report,
        }
    }
}

impl From<SpocFrequencyCaps> for MozAdsSpocFrequencyCaps {
    fn from(caps: SpocFrequencyCaps) -> Self {
        Self {
            cap_key: caps.cap_key,
            day: caps.day,
        }
    }
}

impl From<SpocRanking> for MozAdsSpocRanking {
    fn from(ranking: SpocRanking) -> Self {
        Self {
            item_score: ranking.item_score,
            personalization_models: ranking.personalization_models.unwrap_or_default(),
            priority: ranking.priority,
        }
    }
}

impl From<AdImage> for MozAdsImage {
    fn from(img: AdImage) -> Self {
        Self {
            alt_text: img.alt_text,
            block_key: img.block_key,
            callbacks: img.callbacks.into(),
            format: img.format,
            image_url: img.image_url,
            url: img.url,
        }
    }
}

impl From<AdSpoc> for MozAdsSpoc {
    fn from(spoc: AdSpoc) -> Self {
        Self {
            block_key: spoc.block_key,
            callbacks: spoc.callbacks.into(),
            caps: spoc.caps.into(),
            domain: spoc.domain,
            excerpt: spoc.excerpt,
            format: spoc.format,
            image_url: spoc.image_url,
            ranking: spoc.ranking.into(),
            sponsor: spoc.sponsor,
            sponsored_by_override: spoc.sponsored_by_override,
            title: spoc.title,
            url: spoc.url,
        }
    }
}

impl From<AdTile> for MozAdsTile {
    fn from(tile: AdTile) -> Self {
        Self {
            block_key: tile.block_key,
            callbacks: tile.callbacks.into(),
            format: tile.format,
            image_url: tile.image_url,
            name: tile.name,
            url: tile.url,
        }
    }
}

impl From<Environment> for MozAdsEnvironment {
    fn from(env: Environment) -> Self {
        match env {
            Environment::Prod => MozAdsEnvironment::Prod,
            Environment::Staging => MozAdsEnvironment::Staging,
            #[cfg(test)]
            Environment::Test => MozAdsEnvironment::Test,
        }
    }
}

impl From<MozAdsEnvironment> for Environment {
    fn from(env: MozAdsEnvironment) -> Self {
        match env {
            MozAdsEnvironment::Prod => Environment::Prod,
            MozAdsEnvironment::Staging => Environment::Staging,
            #[cfg(test)]
            MozAdsEnvironment::Test => Environment::Test,
        }
    }
}

impl From<IABContentTaxonomy> for MozAdsIABContentTaxonomy {
    fn from(taxonomy: IABContentTaxonomy) -> Self {
        match taxonomy {
            IABContentTaxonomy::IAB1_0 => MozAdsIABContentTaxonomy::IAB1_0,
            IABContentTaxonomy::IAB2_0 => MozAdsIABContentTaxonomy::IAB2_0,
            IABContentTaxonomy::IAB2_1 => MozAdsIABContentTaxonomy::IAB2_1,
            IABContentTaxonomy::IAB2_2 => MozAdsIABContentTaxonomy::IAB2_2,
            IABContentTaxonomy::IAB3_0 => MozAdsIABContentTaxonomy::IAB3_0,
        }
    }
}

impl From<MozAdsIABContentTaxonomy> for IABContentTaxonomy {
    fn from(taxonomy: MozAdsIABContentTaxonomy) -> Self {
        match taxonomy {
            MozAdsIABContentTaxonomy::IAB1_0 => IABContentTaxonomy::IAB1_0,
            MozAdsIABContentTaxonomy::IAB2_0 => IABContentTaxonomy::IAB2_0,
            MozAdsIABContentTaxonomy::IAB2_1 => IABContentTaxonomy::IAB2_1,
            MozAdsIABContentTaxonomy::IAB2_2 => IABContentTaxonomy::IAB2_2,
            MozAdsIABContentTaxonomy::IAB3_0 => IABContentTaxonomy::IAB3_0,
        }
    }
}

impl From<MozAdsCachePolicy> for CachePolicy {
    fn from(policy: MozAdsCachePolicy) -> Self {
        let ttl = policy.ttl_seconds.map(std::time::Duration::from_secs);
        match policy.mode {
            MozAdsCacheMode::CacheFirst => CachePolicy::CacheFirst { ttl },
            MozAdsCacheMode::NetworkFirst => CachePolicy::NetworkFirst { ttl },
        }
    }
}

impl From<&MozAdsIABContent> for AdContentCategory {
    fn from(content: &MozAdsIABContent) -> Self {
        Self {
            categories: content.category_ids.clone(),
            taxonomy: content.taxonomy.into(),
        }
    }
}

impl From<MozAdsRequestOptions> for CachePolicy {
    fn from(options: MozAdsRequestOptions) -> Self {
        options.cache_policy.map(Into::into).unwrap_or_default()
    }
}

impl From<Option<MozAdsRequestOptions>> for CachePolicy {
    fn from(options: Option<MozAdsRequestOptions>) -> Self {
        options.map(Into::into).unwrap_or_default()
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
            content: request.iab_content.as_ref().map(Into::into),
            count: 1,
            placement: request.placement_id.clone(),
        }
    }
}

impl From<&MozAdsPlacementRequestWithCount> for AdPlacementRequest {
    fn from(request: &MozAdsPlacementRequestWithCount) -> Self {
        Self {
            content: request.iab_content.as_ref().map(Into::into),
            count: request.count,
            placement: request.placement_id.clone(),
        }
    }
}
