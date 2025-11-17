/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;
use std::time::Duration;

use crate::client::ad_response::{AdImage, AdResponse, AdSpoc, AdTile};
use crate::client::config::AdsClientConfig;
use crate::error::{RecordClickError, RecordImpressionError, ReportAdError, RequestAdsError};
use crate::http_cache::{HttpCache, RequestCachePolicy};
use crate::mars::MARSClient;
use ad_request::{AdPlacementRequest, AdRequest};
use context_id::{ContextIDComponent, DefaultContextIdCallback};
use serde::de::DeserializeOwned;
use url::Url;
use uuid::Uuid;

use crate::http_cache::{ByteSize, HttpCacheError};

pub mod ad_request;
pub mod ad_response;
pub mod config;

const DEFAULT_TTL_SECONDS: u64 = 300;
const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;

pub struct AdsClient {
    client: MARSClient,
    context_id_component: ContextIDComponent,
}

impl AdsClient {
    pub fn new(client_config: Option<AdsClientConfig>) -> Self {
        let context_id = Uuid::new_v4().to_string();

        let client_config = client_config.unwrap_or_default();

        let context_id_component = ContextIDComponent::new(
            &context_id,
            0,
            cfg!(test),
            Box::new(DefaultContextIdCallback),
        );

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

            let client = MARSClient::new(client_config.environment, http_cache);
            return Self {
                context_id_component,
                client,
            };
        }

        let client = MARSClient::new(client_config.environment, None);
        Self {
            context_id_component,
            client,
        }
    }

    fn request_ads<T>(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<RequestCachePolicy>,
    ) -> Result<AdResponse<T>, RequestAdsError>
    where
        T: DeserializeOwned,
    {
        let context_id = self.get_context_id()?;
        let ad_request = AdRequest::build(context_id, ad_placement_requests)?;
        let cache_policy = options.unwrap_or_default();
        let response = self.client.fetch_ads(&ad_request, &cache_policy)?;
        Ok(response)
    }

    pub fn request_image_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<RequestCachePolicy>,
    ) -> Result<HashMap<String, AdImage>, RequestAdsError> {
        let response = self.request_ads::<AdImage>(ad_placement_requests, options)?;
        Ok(response.take_first())
    }

    pub fn request_spoc_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<RequestCachePolicy>,
    ) -> Result<HashMap<String, Vec<AdSpoc>>, RequestAdsError> {
        let response = self.request_ads::<AdSpoc>(ad_placement_requests, options)?;
        Ok(response.data)
    }

    pub fn request_tile_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<RequestCachePolicy>,
    ) -> Result<HashMap<String, AdTile>, RequestAdsError> {
        let response = self.request_ads::<AdTile>(ad_placement_requests, options)?;
        Ok(response.take_first())
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

    pub fn get_context_id(&self) -> context_id::ApiResult<String> {
        self.context_id_component.request(0)
    }

    pub fn cycle_context_id(&mut self) -> context_id::ApiResult<String> {
        let old_context_id = self.get_context_id()?;
        self.context_id_component.force_rotation()?;
        Ok(old_context_id)
    }

    pub fn clear_cache(&self) -> Result<(), HttpCacheError> {
        self.client.clear_cache()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{
        get_example_happy_image_response, get_example_happy_spoc_response,
        get_example_happy_uatile_response, make_happy_placement_requests,
    };

    use super::*;

    #[test]
    fn test_get_context_id() {
        let client = AdsClient::new(None);
        let context_id = client.get_context_id().unwrap();
        assert!(!context_id.is_empty());
    }

    #[test]
    fn test_cycle_context_id() {
        let mut client = AdsClient::new(None);
        let old_id = client.get_context_id().unwrap();
        let previous_id = client.cycle_context_id().unwrap();
        assert_eq!(previous_id, old_id);
        let new_id = client.get_context_id().unwrap();
        assert_ne!(new_id, old_id);
    }

    #[test]
    fn test_request_image_ads_happy() {
        use crate::test_utils::create_test_client;
        use context_id::{ContextIDComponent, DefaultContextIdCallback};
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_image_response();
        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response).unwrap())
            .create();

        let mars_client = create_test_client(mockito::server_url());
        let context_id_component = ContextIDComponent::new(
            &uuid::Uuid::new_v4().to_string(),
            0,
            false,
            Box::new(DefaultContextIdCallback),
        );
        let component = AdsClient {
            context_id_component,
            client: mars_client,
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_image_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_request_spocs_happy() {
        use crate::test_utils::create_test_client;
        use context_id::{ContextIDComponent, DefaultContextIdCallback};
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_spoc_response();
        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response).unwrap())
            .create();

        let mars_client = create_test_client(mockito::server_url());
        let context_id_component = ContextIDComponent::new(
            &uuid::Uuid::new_v4().to_string(),
            0,
            false,
            Box::new(DefaultContextIdCallback),
        );
        let component = AdsClient {
            context_id_component,
            client: mars_client,
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_spoc_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_request_tiles_happy() {
        use crate::test_utils::create_test_client;
        use context_id::{ContextIDComponent, DefaultContextIdCallback};
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_uatile_response();
        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response).unwrap())
            .create();

        let mars_client = create_test_client(mockito::server_url());
        let context_id_component = ContextIDComponent::new(
            &uuid::Uuid::new_v4().to_string(),
            0,
            false,
            Box::new(DefaultContextIdCallback),
        );
        let component = AdsClient {
            context_id_component,
            client: mars_client,
        };

        let ad_placement_requests = make_happy_placement_requests();

        let result = component.request_tile_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }
}
