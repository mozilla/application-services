/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;
use std::time::Duration;

use crate::http_cache::{ByteSize, CachePolicy, HttpCache};
use crate::mars::ad_request::AdPlacementRequest;
use crate::mars::ad_response::{AdImage, AdResponse, AdResponseValue, AdSpoc, AdTile};
use crate::mars::error::{RecordClickError, RecordImpressionError, ReportAdError};
use crate::mars::{MARSClient, ReportReason};
use crate::telemetry::Telemetry;
use config::AdsClientConfig;
use context_id::{ContextIDComponent, DefaultContextIdCallback};
use error::RequestAdsError;
use url::Url;
use uuid::Uuid;

pub mod config;
pub mod error;

const DEFAULT_TTL_SECONDS: u64 = 300;
const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;
const DEFAULT_ROTATION_DAYS: u8 = 3;

pub trait ContextIdProvider: Send + Sync {
    fn context_id(&self) -> context_id::ApiResult<String>;
}

impl ContextIdProvider for ContextIDComponent {
    fn context_id(&self) -> context_id::ApiResult<String> {
        self.request(DEFAULT_ROTATION_DAYS)
    }
}

pub struct AdsClient<T>
where
    T: Clone + Telemetry,
{
    client: MARSClient<T>,
    context_id_provider: Box<dyn ContextIdProvider>,
    telemetry: T,
}

impl<T> AdsClient<T>
where
    T: Clone + Telemetry,
{
    pub fn new(client_config: AdsClientConfig<T>) -> Self {
        let context_id_provider = client_config.context_id_provider.unwrap_or_else(|| {
            Box::new(ContextIDComponent::new(
                &Uuid::new_v4().to_string(),
                0,
                cfg!(test),
                Box::new(DefaultContextIdCallback),
            ))
        });

        let telemetry = client_config.telemetry;
        let environment = client_config.environment;

        // Configure the cache if a path is provided.
        // Defaults for ttl and cache size are also set if unspecified.
        let http_cache = client_config.cache_config.and_then(|cache_cfg| {
            let default_cache_ttl = Duration::from_secs(
                cache_cfg
                    .default_cache_ttl_seconds
                    .unwrap_or(DEFAULT_TTL_SECONDS),
            );
            let max_cache_size =
                ByteSize::mib(cache_cfg.max_size_mib.unwrap_or(DEFAULT_MAX_CACHE_SIZE_MIB));

            match HttpCache::builder(cache_cfg.db_path)
                .max_size(max_cache_size)
                .default_ttl(default_cache_ttl)
                .build()
            {
                Ok(cache) => Some(cache),
                Err(e) => {
                    telemetry.record(&e);
                    None
                }
            }
        });

        let client = MARSClient::new(environment, http_cache, telemetry.clone());
        telemetry.record(&ClientOperationEvent::New);
        Self {
            client,
            context_id_provider,
            telemetry: telemetry.clone(),
        }
    }

    pub fn clear_cache(&self) -> Result<(), rusqlite::Error> {
        self.client.clear_cache()
    }

    pub fn get_context_id(&self) -> context_id::ApiResult<String> {
        self.context_id_provider.context_id()
    }

    pub fn record_click(&self, click_url: Url, ohttp: bool) -> Result<(), RecordClickError> {
        // TODO: Re-enable cache invalidation behind a Nimbus experiment.
        // The mobile team has requested this be temporarily disabled.
        // let mut click_url = click_url.clone();
        // if let Some(request_hash) = pop_request_hash_from_url(&mut click_url) {
        //     let _ = self.client.invalidate_cache_by_hash(&request_hash);
        // }
        self.client
            .record_click(click_url, ohttp)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry.record(&ClientOperationEvent::RecordClick);
            })
    }

    pub fn record_impression(
        &self,
        impression_url: Url,
        ohttp: bool,
    ) -> Result<(), RecordImpressionError> {
        // TODO: Re-enable cache invalidation behind a Nimbus experiment.
        // The mobile team has requested this be temporarily disabled.
        // let mut impression_url = impression_url.clone();
        // if let Some(request_hash) = pop_request_hash_from_url(&mut impression_url) {
        //     let _ = self.client.invalidate_cache_by_hash(&request_hash);
        // }
        self.client
            .record_impression(impression_url, ohttp)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry
                    .record(&ClientOperationEvent::RecordImpression);
            })
    }

    pub fn report_ad(
        &self,
        report_url: Url,
        reason: ReportReason,
        ohttp: bool,
    ) -> Result<(), ReportAdError> {
        self.client
            .report_ad(report_url, reason, ohttp)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry.record(&ClientOperationEvent::ReportAd);
            })
    }

    pub fn request_image_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<CachePolicy>,
        ohttp: bool,
    ) -> Result<HashMap<String, AdImage>, RequestAdsError> {
        let response = self
            .request_ads::<AdImage>(ad_placement_requests, options, ohttp)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })?;
        self.telemetry.record(&ClientOperationEvent::RequestAds);
        Ok(response.take_first())
    }

    pub fn request_spoc_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<CachePolicy>,
        ohttp: bool,
    ) -> Result<HashMap<String, Vec<AdSpoc>>, RequestAdsError> {
        let result = self.request_ads::<AdSpoc>(ad_placement_requests, options, ohttp);
        result
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .map(|response| {
                self.telemetry.record(&ClientOperationEvent::RequestAds);
                response.data
            })
    }

    pub fn request_tile_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<CachePolicy>,
        ohttp: bool,
    ) -> Result<HashMap<String, AdTile>, RequestAdsError> {
        let result = self.request_ads::<AdTile>(ad_placement_requests, options, ohttp);
        result
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .map(|response| {
                self.telemetry.record(&ClientOperationEvent::RequestAds);
                response.take_first()
            })
    }

    fn request_ads<A>(
        &self,
        placements: Vec<AdPlacementRequest>,
        options: Option<CachePolicy>,
        ohttp: bool,
    ) -> Result<AdResponse<A>, RequestAdsError>
    where
        A: AdResponseValue,
    {
        let context_id = self.get_context_id()?;
        let cache_policy = options.unwrap_or_default();
        let (mut response, request_hash) =
            self.client
                .fetch_ads::<A>(context_id, placements, cache_policy, ohttp)?;
        response.enrich_callbacks(&request_hash);
        Ok(response)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientOperationEvent {
    New,
    RecordClick,
    RecordImpression,
    ReportAd,
    RequestAds,
}

#[cfg(test)]
mod tests {
    use crate::{
        ffi::telemetry::MozAdsTelemetryWrapper,
        mars::Environment,
        test_utils::{
            get_example_happy_image_response, get_example_happy_spoc_response,
            get_example_happy_uatile_response, make_happy_placement_requests,
        },
    };

    use super::*;

    fn new_with_mars_client(
        client: MARSClient<MozAdsTelemetryWrapper>,
    ) -> AdsClient<MozAdsTelemetryWrapper> {
        AdsClient {
            client,
            context_id_provider: Box::new(ContextIDComponent::new(
                &Uuid::new_v4().to_string(),
                0,
                false,
                Box::new(DefaultContextIdCallback),
            )),
            telemetry: MozAdsTelemetryWrapper::noop(),
        }
    }

    #[test]
    fn test_get_context_id() {
        let config = AdsClientConfig {
            cache_config: None,
            context_id_provider: None,
            environment: Environment::Test,
            telemetry: MozAdsTelemetryWrapper::noop(),
        };
        let client = AdsClient::new(config);
        let context_id = client.get_context_id().unwrap();
        assert!(!context_id.is_empty());
    }

    #[test]
    fn test_request_image_ads_happy() {
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_image_response();
        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let ads_client = new_with_mars_client(mars_client);

        let result = ads_client.request_image_ads(make_happy_placement_requests(), None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_request_spocs_happy() {
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_spoc_response();
        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let ads_client = new_with_mars_client(mars_client);

        let result = ads_client.request_spoc_ads(make_happy_placement_requests(), None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_request_tiles_happy() {
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_uatile_response();
        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let ads_client = new_with_mars_client(mars_client);

        let result = ads_client.request_tile_ads(make_happy_placement_requests(), None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_custom_context_id_provider() {
        viaduct_dev::init_backend_dev();

        struct FixedContextId;
        impl ContextIdProvider for FixedContextId {
            fn context_id(&self) -> context_id::ApiResult<String> {
                Ok("custom-context-id-12345".to_string())
            }
        }

        let expected_response = get_example_happy_image_response();
        let _m = mockito::mock("POST", "/ads")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"context_id":"custom-context-id-12345"}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let config = AdsClientConfig {
            cache_config: None,
            context_id_provider: Some(Box::new(FixedContextId)),
            environment: Environment::Test,
            telemetry: MozAdsTelemetryWrapper::noop(),
        };
        let client = AdsClient::new(config);

        assert_eq!(client.get_context_id().unwrap(), "custom-context-id-12345");

        let result = client.request_image_ads(make_happy_placement_requests(), None, false);
        assert!(result.is_ok());
    }

    #[test]
    #[ignore = "Cache invalidation temporarily disabled - will be re-enabled behind Nimbus experiment"]
    fn test_record_click_invalidates_cache() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_click_invalidates_cache")
            .build()
            .unwrap();
        let mars_client = MARSClient::new(
            Environment::Test,
            Some(cache),
            MozAdsTelemetryWrapper::noop(),
        );
        let ads_client = new_with_mars_client(mars_client);

        let response = get_example_happy_image_response();

        let _m1 = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&response.data).unwrap())
            .expect(2) // we expect 2 requests to the server, one for the initial ad request and one after for the cache invalidation request
            .create();

        let response = ads_client
            .request_image_ads(make_happy_placement_requests(), None, false)
            .unwrap();
        let callback_url = response.values().next().unwrap().callbacks.click.clone();

        let _m2 = mockito::mock("GET", callback_url.path())
            .with_status(200)
            .create();

        ads_client
            .request_image_ads(make_happy_placement_requests(), None, false)
            .unwrap();

        ads_client.record_click(callback_url, false).unwrap();

        ads_client
            .request_ads::<AdImage>(
                make_happy_placement_requests(),
                Some(CachePolicy::default()),
                false,
            )
            .unwrap();
    }
}
