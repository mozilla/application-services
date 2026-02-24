/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use crate::client::ad_response::{AdImage, AdResponse, AdResponseValue, AdSpoc, AdTile};
use crate::error::{RecordClickError, RecordImpressionError, ReportAdError, RequestAdsError};
use crate::http_cache::{HttpCacheError, RequestCachePolicy};
use crate::mars::MARSClient;
use crate::telemetry::Telemetry;
use ad_request::{AdPlacementRequest, AdRequest};
use context_id::ContextIDComponent;
use url::Url;

pub mod ad_request;
pub mod ad_response;
pub mod builder;
pub mod config;

pub struct AdsClient<T>
where
    T: Clone + Telemetry,
{
    client: MARSClient<T>,
    context_id_component: ContextIDComponent,
    telemetry: T,
}

impl<T> AdsClient<T>
where
    T: Clone + Telemetry,
{
    pub fn new(
        client: MARSClient<T>,
        context_id_component: ContextIDComponent,
        telemetry: T,
    ) -> Self {
        telemetry.record(&ClientOperationEvent::New);
        Self {
            client,
            context_id_component,
            telemetry,
        }
    }

    fn request_ads<A>(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<RequestCachePolicy>,
    ) -> Result<AdResponse<A>, RequestAdsError>
    where
        A: AdResponseValue,
    {
        let context_id = self.get_context_id()?;
        let ad_request = AdRequest::build(context_id, ad_placement_requests)?;
        let cache_policy = options.unwrap_or_default();
        let (mut response, request_hash) =
            self.client.fetch_ads::<A>(&ad_request, &cache_policy)?;
        response.add_request_hash_to_callbacks(&request_hash);
        Ok(response)
    }

    pub fn request_image_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<RequestCachePolicy>,
    ) -> Result<HashMap<String, AdImage>, RequestAdsError> {
        let response = self
            .request_ads::<AdImage>(ad_placement_requests, options)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })?;
        self.telemetry.record(&ClientOperationEvent::RequestAds);
        Ok(response.take_first())
    }

    pub fn request_spoc_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        options: Option<RequestCachePolicy>,
    ) -> Result<HashMap<String, Vec<AdSpoc>>, RequestAdsError> {
        let result = self.request_ads::<AdSpoc>(ad_placement_requests, options);
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
        options: Option<RequestCachePolicy>,
    ) -> Result<HashMap<String, AdTile>, RequestAdsError> {
        let result = self.request_ads::<AdTile>(ad_placement_requests, options);
        result
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .map(|response| {
                self.telemetry.record(&ClientOperationEvent::RequestAds);
                response.take_first()
            })
    }

    pub fn record_impression(&self, impression_url: Url) -> Result<(), RecordImpressionError> {
        // TODO: Re-enable cache invalidation behind a Nimbus experiment.
        // The mobile team has requested this be temporarily disabled.
        // let mut impression_url = impression_url.clone();
        // if let Some(request_hash) = pop_request_hash_from_url(&mut impression_url) {
        //     let _ = self.client.invalidate_cache_by_hash(&request_hash);
        // }
        self.client
            .record_impression(impression_url)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry
                    .record(&ClientOperationEvent::RecordImpression);
            })
    }

    pub fn record_click(&self, click_url: Url) -> Result<(), RecordClickError> {
        // TODO: Re-enable cache invalidation behind a Nimbus experiment.
        // The mobile team has requested this be temporarily disabled.
        // let mut click_url = click_url.clone();
        // if let Some(request_hash) = pop_request_hash_from_url(&mut click_url) {
        //     let _ = self.client.invalidate_cache_by_hash(&request_hash);
        // }
        self.client
            .record_click(click_url)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry.record(&ClientOperationEvent::RecordClick);
            })
    }

    pub fn report_ad(&self, report_url: Url) -> Result<(), ReportAdError> {
        self.client
            .report_ad(report_url)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry.record(&ClientOperationEvent::ReportAd);
            })
    }

    pub fn get_context_id(&self) -> context_id::ApiResult<String> {
        self.context_id_component.request(3)
    }

    pub fn clear_cache(&self) -> Result<(), HttpCacheError> {
        self.client.clear_cache()
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
    use context_id::DefaultContextIdCallback;

    use crate::{
        client::config::Environment,
        ffi::telemetry::MozAdsTelemetryWrapper,
        test_utils::{
            get_example_happy_image_response, get_example_happy_spoc_response,
            get_example_happy_uatile_response, make_happy_placement_requests,
        },
    };

    use super::*;

    fn new_with_mars_client(
        client: MARSClient<MozAdsTelemetryWrapper>,
    ) -> AdsClient<MozAdsTelemetryWrapper> {
        let context_id_component = ContextIDComponent::new(
            &uuid::Uuid::new_v4().to_string(),
            0,
            false,
            Box::new(DefaultContextIdCallback),
        );
        AdsClient::new(client, context_id_component, MozAdsTelemetryWrapper::noop())
    }

    #[test]
    fn test_get_context_id() {
        let telemetry = MozAdsTelemetryWrapper::noop();
        let mars_client = MARSClient::new(Environment::Test, None, telemetry.clone());
        let context_id_component = ContextIDComponent::new(
            &uuid::Uuid::new_v4().to_string(),
            0,
            false,
            Box::new(DefaultContextIdCallback),
        );
        let client = AdsClient::new(mars_client, context_id_component, telemetry);
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

        let telemetry = MozAdsTelemetryWrapper::noop();
        let mars_client = MARSClient::new(Environment::Test, None, telemetry);
        let ads_client = new_with_mars_client(mars_client);

        let ad_placement_requests = make_happy_placement_requests();

        let result = ads_client.request_image_ads(ad_placement_requests, None);

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

        let telemetry = MozAdsTelemetryWrapper::noop();
        let mars_client = MARSClient::new(Environment::Test, None, telemetry);
        let ads_client = new_with_mars_client(mars_client);

        let ad_placement_requests = make_happy_placement_requests();

        let result = ads_client.request_spoc_ads(ad_placement_requests, None);

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

        let telemetry = MozAdsTelemetryWrapper::noop();
        let mars_client = MARSClient::new(Environment::Test, None, telemetry.clone());
        let ads_client = new_with_mars_client(mars_client);

        let ad_placement_requests = make_happy_placement_requests();

        let result = ads_client.request_tile_ads(ad_placement_requests, None);

        assert!(result.is_ok());
    }

    #[test]
    #[ignore = "Cache invalidation temporarily disabled - will be re-enabled behind Nimbus experiment"]
    fn test_record_click_invalidates_cache() {
        use crate::http_cache::HttpCache;

        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_click_invalidates_cache")
            .build()
            .unwrap();
        let telemetry = MozAdsTelemetryWrapper::noop();
        let mars_client = MARSClient::new(Environment::Test, Some(cache), telemetry.clone());
        let ads_client = new_with_mars_client(mars_client);

        let response = get_example_happy_image_response();

        let _m1 = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&response.data).unwrap())
            .expect(2) // we expect 2 requests to the server, one for the initial ad request and one after for the cache invalidation request
            .create();

        let response = ads_client
            .request_image_ads(make_happy_placement_requests(), None)
            .unwrap();
        let callback_url = response.values().next().unwrap().callbacks.click.clone();

        let _m2 = mockito::mock("GET", callback_url.path())
            .with_status(200)
            .create();

        // Doing another request should hit the cache
        ads_client
            .request_image_ads(make_happy_placement_requests(), None)
            .unwrap();

        ads_client.record_click(callback_url).unwrap();

        ads_client
            .request_ads::<AdImage>(
                make_happy_placement_requests(),
                Some(RequestCachePolicy::default()),
            )
            .unwrap();
    }
}
