/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{collections::HashMap, time::Duration};

use ad_request::{AdPlacementRequest, AdRequest};
use ad_response::MozAd;
use uuid::Uuid;

use crate::{
    client::config::MozAdsClientConfig,
    error::{
        CallbackRequestError, RecordClickError, RecordImpressionError, ReportAdError,
        RequestAdsError,
    },
    http_cache::{ByteSize, HttpCache, HttpCacheError},
    mars::{DefaultMARSClient, MARSClient},
    MozAdsRequestOptions,
};

pub mod ad_request;
pub mod ad_response;
pub mod config;

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
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> Result<HashMap<String, Vec<MozAd>>, RequestAdsError> {
        let ad_request = AdRequest::build(self.client.get_context_id()?, ad_placement_requests)?;
        let options = options.unwrap_or_default();
        let cache_policy = options.cache_policy.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        let placements = response.build_placements(&ad_request)?;
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

    pub fn clear_cache(&self) -> Result<(), HttpCacheError> {
        self.client.clear_cache()
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;
    use crate::{
        client::ad_request::{AdContentCategory, IABContentTaxonomy},
        mars::MockMARSClient,
        test_utils::{
            get_example_happy_ad_response, get_example_happy_placements,
            make_happy_placement_requests,
        },
    };

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

        let component = MozAdsClientInner {
            client: Box::new(mock),
        };

        let ad_placement_requests: Vec<AdPlacementRequest> = vec![
            AdPlacementRequest {
                placement: "example_placement_1".to_string(),
                count: 1,
                content: Some(AdContentCategory {
                    taxonomy: IABContentTaxonomy::IAB2_1,
                    categories: vec!["entertainment".to_string()],
                }),
            },
            AdPlacementRequest {
                placement: "example_placement_2".to_string(),
                count: 2,
                content: Some(AdContentCategory {
                    taxonomy: IABContentTaxonomy::IAB3_0,
                    categories: vec![],
                }),
            },
        ];

        let result = component.request_ads(ad_placement_requests, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), get_example_happy_placements());
    }
}
