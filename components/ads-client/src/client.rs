/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::time::Duration;

use crate::client::ad_response::AdResponse;
use crate::client::config::AdsClientConfig;
use crate::error::{RecordClickError, RecordImpressionError, ReportAdError, RequestAdsError};
use crate::http_cache::{HttpCache, RequestCachePolicy};
use crate::mars::{DefaultMARSClient, MARSClient};
use ad_request::{AdPlacementRequest, AdRequest};
use url::Url;
use uuid::Uuid;

use crate::http_cache::{ByteSize, HttpCacheError};

pub mod ad_request;
pub mod ad_response;
pub mod config;

const DEFAULT_TTL_SECONDS: u64 = 300;
const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;

pub struct AdsClient {
    client: Box<dyn MARSClient>,
}

impl AdsClient {
    pub fn new(client_config: Option<AdsClientConfig>) -> Self {
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
        options: Option<RequestCachePolicy>,
    ) -> Result<AdResponse, RequestAdsError> {
        let ad_request = AdRequest::build(self.client.get_context_id()?, ad_placement_requests)?;
        let cache_policy = options.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        Ok(response)
    }

    pub fn record_impression(&self, impression_url: Url) -> Result<(), RecordImpressionError> {
        self.client.record_impression(impression_url)
    }

    pub fn record_click(&self, click_url: Url) -> Result<(), RecordClickError> {
        self.client.record_click(click_url)
    }

    pub fn report_ad(&self, report_url: Url) -> Result<(), ReportAdError> {
        self.client.report_ad(report_url)?;
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

    use crate::{
        mars::MockMARSClient,
        test_utils::{
            get_example_happy_image_response, get_example_happy_spoc_response,
            get_example_happy_uatile_response, make_happy_placement_requests,
        },
    };

    use super::*;

    #[test]
    fn test_request_image_ads_happy() {
        let mut mock = MockMARSClient::new();
        mock.expect_fetch_ads()
            .returning(|_req, _| Ok(get_example_happy_image_response()));
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        mock.expect_get_mars_endpoint()
            .return_const(Url::parse("https://mock.endpoint/ads").unwrap());

        let component = AdsClient {
            client: Box::new(mock),
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_request_spocs_happy() {
        let mut mock = MockMARSClient::new();
        mock.expect_fetch_ads()
            .returning(|_req, _| Ok(get_example_happy_spoc_response()));
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        mock.expect_get_mars_endpoint()
            .return_const(Url::parse("https://mock.endpoint/ads").unwrap());

        let component = AdsClient {
            client: Box::new(mock),
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_request_uatiles_happy() {
        let mut mock = MockMARSClient::new();
        mock.expect_fetch_ads()
            .returning(|_req, _| Ok(get_example_happy_uatile_response()));
        mock.expect_get_context_id()
            .returning(|| Ok("mock-context-id".to_string()));

        mock.expect_get_mars_endpoint()
            .return_const(Url::parse("https://mock.endpoint/ads").unwrap());

        let component = AdsClient {
            client: Box::new(mock),
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }
}
