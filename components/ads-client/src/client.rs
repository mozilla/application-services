/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::client::ad_request::{AdPlacementRequest, AdRequest};
use crate::client::ad_response::{AdResponse, MozAd};
use crate::error::{
    BuildPlacementsError, RecordClickError, RecordImpressionError, ReportAdError, RequestAdsError,
};
use crate::http_cache::HttpCache;
use crate::mars::{DefaultMARSClient, MARSClient};
use crate::{
    MozAdsClientConfig, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
    MozAdsRequestOptions,
};
use uuid::Uuid;

use crate::error::CallbackRequestError;
use crate::http_cache::{ByteSize, HttpCacheError};

pub mod ad_request;
pub mod ad_response;

const DEFAULT_TTL_SECONDS: u64 = 300;
const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;

pub struct MozAdsClientInner {
    client: Box<dyn MARSClient>,
}

impl MozAdsClientInner {
    pub fn new(client_config: Option<MozAdsClientConfig>) -> Self {
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

    pub fn request_ads(
        &self,
        moz_ad_requests: &[MozAdsPlacementRequest],
        options: Option<MozAdsRequestOptions>,
    ) -> Result<HashMap<String, MozAd>, RequestAdsError> {
        let ad_placement_requests: Vec<AdPlacementRequest> =
            moz_ad_requests.iter().map(|r| r.into()).collect();
        let ad_request = AdRequest::build(self.client.get_context_id()?, ad_placement_requests)?;
        let options = options.unwrap_or_default();
        let cache_policy = options.cache_policy.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        let placements = self.build_placements(&ad_request, response)?;
        let placements = self.pop_placements(placements);
        Ok(placements)
    }

    pub fn request_ads_multiset(
        &self,
        moz_ad_requests: &[MozAdsPlacementRequestWithCount],
        options: Option<MozAdsRequestOptions>,
    ) -> Result<HashMap<String, Vec<MozAd>>, RequestAdsError> {
        let ad_placement_requests: Vec<AdPlacementRequest> =
            moz_ad_requests.iter().map(|r| r.into()).collect();
        let ad_request = AdRequest::build(self.client.get_context_id()?, ad_placement_requests)?;
        let options = options.unwrap_or_default();
        let cache_policy = options.cache_policy.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        let placements = self.build_placements(&ad_request, response)?;
        Ok(placements)
    }

    pub fn record_impression(&self, placement: &MozAd) -> Result<(), RecordImpressionError> {
        self.client
            .record_impression(placement.callbacks.impression.clone())
    }

    pub fn record_click(&self, placement: &MozAd) -> Result<(), RecordClickError> {
        self.client.record_click(placement.callbacks.click.clone())
    }

    pub fn report_ad(&self, placement: &MozAd) -> Result<(), ReportAdError> {
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

    pub fn cycle_context_id(&mut self) -> context_id::ApiResult<String> {
        self.client.cycle_context_id()
    }

    pub fn build_placements(
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

    pub fn pop_placements(
        &self,
        placements: HashMap<String, Vec<MozAd>>,
    ) -> HashMap<String, MozAd> {
        placements
            .into_iter()
            .filter_map(|(placement_id, mut vec)| {
                vec.pop().map(|placement| (placement_id, placement))
            })
            .collect()
    }

    pub fn clear_cache(&self) -> Result<(), HttpCacheError> {
        self.client.clear_cache()
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::{
        client::{
            ad_request::{AdContentCategory, IABContentTaxonomy},
            ad_response::AdCallbacks,
        },
        mars::MockMARSClient,
        test_utils::{
            get_example_happy_ad_response, get_example_happy_placements,
            make_happy_placement_requests,
        },
        IABContent, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
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
        let context_id = inner_component.client.get_context_id().unwrap();
        let request = AdRequest::build(
            context_id.clone(),
            ad_placement_requests.iter().map(|r| r.into()).collect(),
        )
        .unwrap();

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
        let request = AdRequest::build(
            inner_component.client.get_context_id().unwrap(),
            ad_placement_requests.iter().map(|r| r.into()).collect(),
        );

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
        let request = AdRequest::build(
            inner_component.client.get_context_id().unwrap(),
            ad_placement_requests.iter().map(|r| r.into()).collect(),
        );

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

        let ad_request = AdRequest::build(
            inner_component.client.get_context_id().unwrap(),
            make_happy_placement_requests()
                .iter()
                .map(|r| r.into())
                .collect(),
        )
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

        let ad_request = AdRequest::build(
            inner_component.client.get_context_id().unwrap(),
            ad_placement_requests.iter().map(|r| r.into()).collect(),
        )
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

        let ad_request = AdRequest::build(
            inner_component.client.get_context_id().unwrap(),
            ad_placement_requests.iter().map(|r| r.into()).collect(),
        )
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
                    click: Url::parse("https://ads.fakeexample.org/click/example_ad_2_2").unwrap(),
                    impression: Url::parse("https://ads.fakeexample.org/impression/example_ad_2_2")
                        .unwrap(),
                    report: Some(
                        Url::parse("https://ads.fakeexample.org/report/example_ad_2_2").unwrap(),
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

        let component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_ads(&ad_placement_requests, None);

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

        let component = MozAdsClientInner {
            client: Box::new(mock),
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

        let result = component.request_ads_multiset(&ad_placement_requests, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), get_example_happy_placements());
    }

    #[test]
    fn test_cycle_context_id() {
        let mut component = MozAdsClientInner::new(None);
        let old_id = component.cycle_context_id().unwrap();
        let new_id = component.cycle_context_id().unwrap();
        assert_ne!(old_id, new_id);
    }
}
