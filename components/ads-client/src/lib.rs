/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use error::AdsClientApiResult;
use error::{
    BuildPlacementsError, BuildRequestError, ComponentError, RecordClickError,
    RecordImpressionError, ReportAdError, RequestAdsError,
};
use error_support::handle_error;
use http_cache::HttpCache;
use instrument::TrackError;
use mars::{DefaultMARSClient, MARSClient};
use models::{AdContentCategory, AdRequest, AdResponse, IABContentTaxonomy, MozAd};
use parking_lot::Mutex;
use uuid::Uuid;

use crate::error::AdsClientApiError;
use crate::http_cache::{
    ByteSize, CacheError, CachePolicy, DEFAULT_MAX_CACHE_SIZE_MIB, DEFAULT_TTL_SECONDS,
};

mod error;
mod http_cache;
mod instrument;
mod mars;
mod models;

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

/// Top-level API for the mac component
#[derive(uniffi::Object)]
pub struct MozAdsClient {
    inner: Mutex<MozAdsClientInner>,
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Environment {
    #[default]
    Prod,
    #[cfg(feature = "dev")]
    Staging,
}

#[derive(uniffi::Record)]
pub struct MozAdsClientConfig {
    pub environment: Environment,
    pub cache_config: Option<MozAdsCacheConfig>,
}

#[derive(uniffi::Record)]
pub struct MozAdsCacheConfig {
    pub db_path: String,
    pub default_cache_ttl_seconds: Option<u64>,
    pub max_size_mib: Option<u64>,
}

#[derive(uniffi::Record)]
pub struct MozAdsRequestOptions {
    pub cache_policy: Option<CachePolicy>,
}

impl Default for MozAdsRequestOptions {
    fn default() -> Self {
        Self {
            cache_policy: Some(CachePolicy::default()),
        }
    }
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
        moz_ad_configs: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsPlacement>> {
        let inner = self.inner.lock();

        let placements = inner
            .request_ads(&moz_ad_configs, options)
            .map_err(ComponentError::RequestAds)?;
        Ok(placements)
    }

    #[handle_error(ComponentError)]
    pub fn record_impression(&self, placement: MozAdsPlacement) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .record_impression(&placement)
            .map_err(ComponentError::RecordImpression)
            .emit_telemetry_if_error()
    }

    #[handle_error(ComponentError)]
    pub fn record_click(&self, placement: MozAdsPlacement) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .record_click(&placement)
            .map_err(ComponentError::RecordClick)
            .emit_telemetry_if_error()
    }

    #[handle_error(ComponentError)]
    pub fn report_ad(&self, placement: MozAdsPlacement) -> AdsClientApiResult<()> {
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

pub struct MozAdsClientInner {
    client: Box<dyn MARSClient>,
}

impl MozAdsClientInner {
    fn new(client_config: Option<MozAdsClientConfig>) -> Self {
        let context_id = Uuid::new_v4().to_string();

        let cfg = client_config.unwrap_or(MozAdsClientConfig {
            environment: Environment::default(),
            cache_config: None,
        });

        // Configure the cache if a path is provided.
        // Defaults for ttl and cache size are also set if unspecified.
        if let Some(cache_cfg) = cfg.cache_config {
            let default_cache_ttl = cache_cfg
                .default_cache_ttl_seconds
                .unwrap_or(DEFAULT_TTL_SECONDS);
            let max_cache_size_mib = cache_cfg.max_size_mib.unwrap_or(DEFAULT_MAX_CACHE_SIZE_MIB);

            let http_cache = HttpCache::builder(cache_cfg.db_path)
                .max_size(ByteSize::mib(max_cache_size_mib))
                .ttl(Duration::from_secs(default_cache_ttl))
                .build()
                .ok(); // TODO: handle error with telemetry

            let client = Box::new(DefaultMARSClient::new(
                context_id,
                cfg.environment,
                http_cache,
            ));
            return Self { client };
        }

        let client = Box::new(DefaultMARSClient::new(context_id, cfg.environment, None));
        Self { client }
    }

    fn request_ads(
        &self,
        moz_ad_configs: &Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> Result<HashMap<String, MozAdsPlacement>, RequestAdsError> {
        let ad_request = self.build_request_from_placement_configs(moz_ad_configs)?;
        let opts = options.unwrap_or_default();
        let cache_policy = opts.cache_policy.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        let placements = self.build_placements(moz_ad_configs, response)?;
        Ok(placements)
    }

    fn record_impression(&self, placement: &MozAdsPlacement) -> Result<(), RecordImpressionError> {
        let impression_callback = placement.content.callbacks.impression.clone();

        self.client.record_impression(impression_callback)?;
        Ok(())
    }

    fn record_click(&self, placement: &MozAdsPlacement) -> Result<(), RecordClickError> {
        let click_callback = placement.content.callbacks.click.clone();

        self.client.record_click(click_callback)?;
        Ok(())
    }

    fn report_ad(&self, placement: &MozAdsPlacement) -> Result<(), ReportAdError> {
        let report_ad_callback = placement.content.callbacks.report.clone();

        self.client.report_ad(report_ad_callback)?;
        Ok(())
    }

    fn cycle_context_id(&mut self) -> context_id::ApiResult<String> {
        self.client.cycle_context_id()
    }

    fn build_request_from_placement_configs(
        &self,
        moz_ad_configs: &Vec<MozAdsPlacementRequest>,
    ) -> Result<AdRequest, BuildRequestError> {
        if moz_ad_configs.is_empty() {
            return Err(BuildRequestError::EmptyConfig);
        }

        let context_id = self.client.get_context_id()?;
        let mut request = AdRequest {
            placements: vec![],
            context_id,
        };

        let mut used_placement_ids: HashSet<&String> = HashSet::new();

        for config in moz_ad_configs {
            if used_placement_ids.contains(&config.placement_id) {
                return Err(BuildRequestError::DuplicatePlacementId {
                    placement_id: config.placement_id.clone(),
                });
            }

            request.placements.push(models::AdPlacementRequest {
                placement: config.placement_id.clone(),
                count: 1, // Placement_id should be treated as unique, so count is always 1
                content: config
                    .iab_content
                    .clone()
                    .map(|iab_content| AdContentCategory {
                        categories: iab_content.category_ids,
                        taxonomy: iab_content.taxonomy,
                    }),
            });

            used_placement_ids.insert(&config.placement_id);
        }

        Ok(request)
    }

    fn build_placements(
        &self,
        placement_configs: &Vec<MozAdsPlacementRequest>,
        mut mars_response: AdResponse,
    ) -> Result<HashMap<String, MozAdsPlacement>, BuildPlacementsError> {
        let mut moz_ad_placements: HashMap<String, MozAdsPlacement> = HashMap::new();

        for config in placement_configs {
            let placement_content = mars_response.data.get_mut(&config.placement_id);

            match placement_content {
                Some(v) => {
                    let ad_content = v.pop();
                    match ad_content {
                        Some(c) => {
                            let is_updated = moz_ad_placements.insert(
                                config.placement_id.clone(),
                                MozAdsPlacement {
                                    content: c,
                                    placement_config: config.clone(),
                                },
                            );
                            if let Some(v) = is_updated {
                                return Err(BuildPlacementsError::DuplicatePlacementId {
                                    placement_id: v.placement_config.placement_id,
                                });
                            }
                        }
                        None => continue,
                    }
                }
                None => continue,
            }
        }

        Ok(moz_ad_placements)
    }

    fn clear_cache(&self) -> Result<(), CacheError> {
        self.client.clear_cache()
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

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacement {
    pub placement_config: MozAdsPlacementRequest,
    pub content: MozAd,
}

#[cfg(test)]
mod tests {

    use parking_lot::lock_api::Mutex;

    use crate::{
        mars::MockMARSClient,
        models::{AdCallbacks, AdContentCategory, AdPlacementRequest},
        test_utils::{
            get_example_happy_ad_response, get_example_happy_placement_config,
            get_example_happy_placements,
        },
    };

    use super::*;

    #[test]
    fn test_build_ad_request_happy() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let configs: Vec<MozAdsPlacementRequest> = vec![
            MozAdsPlacementRequest {
                placement_id: "example_placement_1".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB2_1,
                    category_ids: vec!["entertainment".to_string()],
                }),
            },
            MozAdsPlacementRequest {
                placement_id: "example_placement_2".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB3_0,
                    category_ids: vec![],
                }),
            },
            MozAdsPlacementRequest {
                placement_id: "example_placement_3".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB2_1,
                    category_ids: vec![],
                }),
            },
        ];
        let request = inner_component
            .build_request_from_placement_configs(&configs)
            .unwrap();
        let context_id = inner_component.client.get_context_id().unwrap();

        let expected_request = AdRequest {
            context_id,
            placements: vec![
                AdPlacementRequest {
                    placement: "example_placement_1".to_string(),
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB2_1,
                        categories: vec!["entertainment".to_string()],
                    }),
                    count: 1,
                },
                AdPlacementRequest {
                    placement: "example_placement_2".to_string(),
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB3_0,
                        categories: vec![],
                    }),
                    count: 1,
                },
                AdPlacementRequest {
                    placement: "example_placement_3".to_string(),
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB2_1,
                        categories: vec![],
                    }),
                    count: 1,
                },
            ],
        };

        assert_eq!(request, expected_request);
    }

    #[test]
    fn test_build_ad_request_fails_on_duplicate_placement_id() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let configs: Vec<MozAdsPlacementRequest> = vec![
            MozAdsPlacementRequest {
                placement_id: "example_placement_1".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB2_1,
                    category_ids: vec!["entertainment".to_string()],
                }),
            },
            MozAdsPlacementRequest {
                placement_id: "example_placement_2".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB3_0,
                    category_ids: vec![],
                }),
            },
            MozAdsPlacementRequest {
                placement_id: "example_placement_2".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB2_1,
                    category_ids: vec![],
                }),
            },
        ];
        let request = inner_component.build_request_from_placement_configs(&configs);

        assert!(request.is_err());
    }

    #[test]
    fn test_build_ad_request_fails_on_empty_configs() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let configs: Vec<MozAdsPlacementRequest> = vec![];
        let request = inner_component.build_request_from_placement_configs(&configs);

        assert!(request.is_err());
    }

    #[test]
    fn test_build_placements_happy() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let placements = inner_component
            .build_placements(
                &get_example_happy_placement_config(),
                get_example_happy_ad_response(),
            )
            .unwrap();

        assert_eq!(placements, get_example_happy_placements());
    }

    #[test]
    fn test_build_placements_with_empty_placement_in_response() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let mut configs = get_example_happy_placement_config();
        // Adding an extra placement config
        configs.push(MozAdsPlacementRequest {
            placement_id: "example_placement_3".to_string(),
            iab_content: Some(IABContent {
                taxonomy: IABContentTaxonomy::IAB2_1,
                category_ids: vec![],
            }),
        });

        let mut api_resp = get_example_happy_ad_response();
        api_resp
            .data
            .insert("example_placement_3".to_string(), vec![]);

        let placements = inner_component
            .build_placements(&configs, api_resp)
            .unwrap();

        assert_eq!(placements, get_example_happy_placements());
    }

    #[test]
    fn test_request_ads_with_missing_callback_in_response() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let mut configs = get_example_happy_placement_config();
        // Adding an extra placement config
        configs.push(MozAdsPlacementRequest {
            placement_id: "example_placement_3".to_string(),
            iab_content: Some(IABContent {
                taxonomy: IABContentTaxonomy::IAB2_1,
                category_ids: vec![],
            }),
        });

        let placements = inner_component
            .build_placements(&configs, get_example_happy_ad_response())
            .unwrap();

        assert_eq!(placements, get_example_happy_placements());
    }

    #[test]
    fn test_build_placements_fails_with_duplicate_placement() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let mut configs = get_example_happy_placement_config();
        // Adding an extra placement config
        configs.push(MozAdsPlacementRequest {
            placement_id: "example_placement_2".to_string(),
            iab_content: Some(IABContent {
                taxonomy: IABContentTaxonomy::IAB2_1,
                category_ids: vec![],
            }),
        });

        let mut api_resp = get_example_happy_ad_response();

        // Adding an extra placement in response to match extra config
        api_resp
            .data
            .get_mut("example_placement_2")
            .unwrap()
            .push(MozAd {
                url: "https://ads.fakeexample.org/example_ad_2_2".to_string(),
                image_url: "https://ads.fakeexample.org/example_image_2_2".to_string(),
                format: "skyscraper".to_string(),
                block_key: "abc123".into(),
                alt_text: Some("An ad for a pet dragon".to_string()),
                callbacks: AdCallbacks {
                    click: Some("https://ads.fakeexample.org/click/example_ad_2_2".to_string()),
                    impression: Some(
                        "https://ads.fakeexample.org/impression/example_ad_2_2".to_string(),
                    ),
                    report: Some("https://ads.fakeexample.org/report/example_ad_2_2".to_string()),
                },
            });

        let placements = inner_component.build_placements(&configs, api_resp);

        assert!(placements.is_err());
    }

    #[test]
    fn test_request_ads_happy() {
        let mut mock = MockMARSClient::new();
        mock.expect_fetch_ads()
            .returning(|_req, _| Ok(get_example_happy_ad_response()));
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        mock.expect_get_mars_endpoint()
            .return_const("https://mock.endpoint/ads".to_string());

        let component = MozAdsClient {
            inner: Mutex::new(MozAdsClientInner {
                client: Box::new(mock),
            }),
        };

        let configs = get_example_happy_placement_config();

        let result = component.request_ads(configs, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_cycle_context_id() {
        let component = MozAdsClient::new(None);
        let old_id = component.cycle_context_id().unwrap();
        let new_id = component.cycle_context_id().unwrap();
        assert_ne!(old_id, new_id);
    }
}
