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
use url::Url as AdsClientUrl;
use uuid::Uuid;

use crate::error::{AdsClientApiError, CallbackRequestError};
use crate::http_cache::{ByteSize, CacheMode, HttpCacheError, RequestCachePolicy};
use crate::models::AdPlacementRequest;

mod error;
mod http_cache;
mod instrument;
mod mars;
mod models;

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

uniffi::custom_type!(AdsClientUrl, String, {
    remote,
    try_lift: |val| Ok(AdsClientUrl::parse(&val)?),
    lower: |obj| obj.as_str().to_string(),
});

const DEFAULT_TTL_SECONDS: u64 = 300;
const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;

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

#[derive(uniffi::Record, Default)]
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

        let placements = inner
            .request_ads(&moz_ad_requests, options)
            .map_err(ComponentError::RequestAds)?;
        Ok(placements)
    }

    #[handle_error(ComponentError)]
    pub fn request_ads_multiset(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAd>>> {
        let inner = self.inner.lock();
        let placements = inner
            .request_ads_multiset(&moz_ad_requests, options)
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

pub struct MozAdsClientInner {
    client: Box<dyn MARSClient>,
}

impl MozAdsClientInner {
    fn new(client_config: Option<MozAdsClientConfig>) -> Self {
        let context_id = Uuid::new_v4().to_string();

        let client_config = client_config.unwrap_or_default();

        // Configure the cache if a path is provided.
        // Defaults for ttl and cache size are also set if unspecified.
        if let Some(cache_cfg) = client_config.cache_config {
            let default_cache_ttl = Duration::from_secs(
                cache_cfg
                    .default_cache_ttl_seconds
                    .unwrap_or(DEFAULT_TTL_SECONDS),
            );
            let max_cache_size =
                ByteSize::mib(cache_cfg.max_size_mib.unwrap_or(DEFAULT_MAX_CACHE_SIZE_MIB));

            let http_cache = HttpCache::builder(cache_cfg.db_path)
                .max_size(max_cache_size)
                .default_ttl(default_cache_ttl)
                .build()
                .ok(); // TODO: handle error with telemetry

            let client = Box::new(DefaultMARSClient::new(
                context_id,
                client_config.environment,
                http_cache,
            ));
            return Self { client };
        }

        let client = Box::new(DefaultMARSClient::new(
            context_id,
            client_config.environment,
            None,
        ));
        Self { client }
    }

    fn request_ads(
        &self,
        moz_ad_requests: &[MozAdsPlacementRequest],
        options: Option<MozAdsRequestOptions>,
    ) -> Result<HashMap<String, MozAd>, RequestAdsError> {
        let ad_request = self.build_request_from_requested_placements(moz_ad_requests)?;
        let options = options.unwrap_or_default();
        let cache_policy = options.cache_policy.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        let placements = self.build_placements(&ad_request, response)?;
        let placements = self.pop_placements(placements);
        Ok(placements)
    }

    fn request_ads_multiset(
        &self,
        moz_ad_requests: &[MozAdsPlacementRequestWithCount],
        options: Option<MozAdsRequestOptions>,
    ) -> Result<HashMap<String, Vec<MozAd>>, RequestAdsError> {
        let ad_request = self.build_request_from_requested_placements(moz_ad_requests)?;
        let options = options.unwrap_or_default();
        let cache_policy = options.cache_policy.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        let placements = self.build_placements(&ad_request, response)?;
        Ok(placements)
    }

    fn record_impression(&self, placement: &MozAd) -> Result<(), RecordImpressionError> {
        self.client
            .record_impression(placement.callbacks.impression.clone())
    }

    fn record_click(&self, placement: &MozAd) -> Result<(), RecordClickError> {
        self.client.record_click(placement.callbacks.click.clone())
    }

    fn report_ad(&self, placement: &MozAd) -> Result<(), ReportAdError> {
        let report_ad_callback = placement.callbacks.report.clone();

        match report_ad_callback {
            Some(callback) => self.client.report_ad(callback)?,
            None => {
                return Err(ReportAdError::CallbackRequest(
                    CallbackRequestError::MissingCallback {
                        message: "Report callback url empty.".to_string(),
                    },
                ));
            }
        }
        Ok(())
    }

    fn cycle_context_id(&mut self) -> context_id::ApiResult<String> {
        self.client.cycle_context_id()
    }

    fn build_request_from_requested_placements<A>(
        &self,
        ad_placement_requests: &[A],
    ) -> Result<AdRequest, BuildRequestError>
    where
        for<'a> &'a A: Into<AdPlacementRequest>,
    {
        let ad_placement_requests: Vec<AdPlacementRequest> =
            ad_placement_requests.iter().map(|r| r.into()).collect();

        if ad_placement_requests.is_empty() {
            return Err(BuildRequestError::EmptyConfig);
        }

        let context_id = self.client.get_context_id()?;
        let mut request = AdRequest {
            placements: vec![],
            context_id,
        };

        let mut used_placement_ids: HashSet<String> = HashSet::new();

        for ad_placement_request in ad_placement_requests {
            if used_placement_ids.contains(&ad_placement_request.placement) {
                return Err(BuildRequestError::DuplicatePlacementId {
                    placement_id: ad_placement_request.placement.clone(),
                });
            }

            request.placements.push(models::AdPlacementRequest {
                placement: ad_placement_request.placement.clone(),
                count: ad_placement_request.count,
                content: ad_placement_request
                    .content
                    .map(|iab_content| AdContentCategory {
                        categories: iab_content.categories,
                        taxonomy: iab_content.taxonomy,
                    }),
            });

            used_placement_ids.insert(ad_placement_request.placement.clone());
        }

        Ok(request)
    }

    fn build_placements(
        &self,
        ad_request: &AdRequest,
        mut mars_response: AdResponse,
    ) -> Result<HashMap<String, Vec<MozAd>>, BuildPlacementsError> {
        let mut moz_ad_placements: HashMap<String, Vec<MozAd>> = HashMap::new();
        let mut seen_placements: HashSet<String> = HashSet::new();

        for placement_request in &ad_request.placements {
            if seen_placements.contains(&placement_request.placement) {
                return Err(BuildPlacementsError::DuplicatePlacementId {
                    placement_id: placement_request.placement.clone(),
                });
            }
            seen_placements.insert(placement_request.placement.clone());

            let placement_content = mars_response.data.remove(&placement_request.placement);

            if let Some(v) = placement_content {
                if v.is_empty() {
                    continue;
                }
                moz_ad_placements.insert(placement_request.placement.clone(), v);
            }
        }

        Ok(moz_ad_placements)
    }

    fn pop_placements(&self, placements: HashMap<String, Vec<MozAd>>) -> HashMap<String, MozAd> {
        placements
            .into_iter()
            .filter_map(|(placement_id, mut vec)| {
                vec.pop().map(|placement| (placement_id, placement))
            })
            .collect()
    }

    fn clear_cache(&self) -> Result<(), HttpCacheError> {
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

#[cfg(test)]
mod tests {

    use parking_lot::lock_api::Mutex;
    use url::Url;

    use crate::{
        mars::MockMARSClient,
        models::{AdCallbacks, AdContentCategory, AdPlacementRequest},
        test_utils::{
            get_example_happy_ad_response, get_example_happy_placements,
            make_happy_placement_requests,
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

        let ad_placement_requests: Vec<MozAdsPlacementRequest> = vec![
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
            .build_request_from_requested_placements(&ad_placement_requests)
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

        let ad_placement_requests: Vec<MozAdsPlacementRequest> = vec![
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
        let request =
            inner_component.build_request_from_requested_placements(&ad_placement_requests);

        assert!(request.is_err());
    }

    #[test]
    fn test_build_ad_request_fails_on_empty_request() {
        let mut mock = MockMARSClient::new();
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        let inner_component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let ad_placement_requests: Vec<MozAdsPlacementRequest> = vec![];
        let request =
            inner_component.build_request_from_requested_placements(&ad_placement_requests);

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

        let ad_request = inner_component
            .build_request_from_requested_placements(&make_happy_placement_requests())
            .unwrap();

        let placements = inner_component
            .build_placements(&ad_request, get_example_happy_ad_response())
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

        let mut ad_placement_requests = make_happy_placement_requests();
        // Adding an extra placement request
        ad_placement_requests.push(MozAdsPlacementRequest {
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

        let ad_request = inner_component
            .build_request_from_requested_placements(&ad_placement_requests)
            .unwrap();

        let placements = inner_component
            .build_placements(&ad_request, api_resp)
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

        let mut ad_placement_requests = make_happy_placement_requests();
        // Adding an extra placement request
        ad_placement_requests.push(MozAdsPlacementRequest {
            placement_id: "example_placement_3".to_string(),
            iab_content: Some(IABContent {
                taxonomy: IABContentTaxonomy::IAB2_1,
                category_ids: vec![],
            }),
        });

        let ad_request = inner_component
            .build_request_from_requested_placements(&ad_placement_requests)
            .unwrap();

        let placements = inner_component
            .build_placements(&ad_request, get_example_happy_ad_response())
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

        let mut api_resp = get_example_happy_ad_response();

        // Adding an extra placement in response for the duplicate placement id
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
                    click: AdsClientUrl::parse("https://ads.fakeexample.org/click/example_ad_2_2")
                        .unwrap(),
                    impression: AdsClientUrl::parse(
                        "https://ads.fakeexample.org/impression/example_ad_2_2",
                    )
                    .unwrap(),
                    report: Some(
                        AdsClientUrl::parse("https://ads.fakeexample.org/report/example_ad_2_2")
                            .unwrap(),
                    ),
                },
            });

        // Manually construct an AdRequest with a duplicate placement id to trigger the error
        let ad_request = AdRequest {
            context_id: "mock-context-id".to_string(),
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
                    placement: "example_placement_2".to_string(),
                    content: Some(AdContentCategory {
                        taxonomy: IABContentTaxonomy::IAB2_1,
                        categories: vec![],
                    }),
                    count: 1,
                },
            ],
        };

        let placements = inner_component.build_placements(&ad_request, api_resp);

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
            .return_const(Url::parse("https://mock.endpoint/ads").unwrap());

        let component = MozAdsClient {
            inner: Mutex::new(MozAdsClientInner {
                client: Box::new(mock),
            }),
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_request_ads_multiset_happy() {
        let mut mock = MockMARSClient::new();
        mock.expect_fetch_ads()
            .returning(|_req, _| Ok(get_example_happy_ad_response()));
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        mock.expect_get_mars_endpoint()
            .return_const(Url::parse("https://mock.endpoint/ads").unwrap());

        let component = MozAdsClient {
            inner: Mutex::new(MozAdsClientInner {
                client: Box::new(mock),
            }),
        };

        let ad_placement_requests: Vec<MozAdsPlacementRequestWithCount> = vec![
            MozAdsPlacementRequestWithCount {
                count: 1,
                placement_id: "example_placement_1".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB2_1,
                    category_ids: vec!["entertainment".to_string()],
                }),
            },
            MozAdsPlacementRequestWithCount {
                count: 1,
                placement_id: "example_placement_2".to_string(),
                iab_content: Some(IABContent {
                    taxonomy: IABContentTaxonomy::IAB3_0,
                    category_ids: vec![],
                }),
            },
        ];

        let result = component.request_ads_multiset(ad_placement_requests, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), get_example_happy_placements());
    }

    #[test]
    fn test_cycle_context_id() {
        let component = MozAdsClient::new(None);
        let old_id = component.cycle_context_id().unwrap();
        let new_id = component.cycle_context_id().unwrap();
        assert_ne!(old_id, new_id);
    }
}
