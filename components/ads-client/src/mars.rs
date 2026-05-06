/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod ad_request;
pub mod ad_response;
pub mod environment;
pub mod error;
mod preflight;
pub mod report_reason;
mod transport;

pub use environment::Environment;
pub use report_reason::ReportReason;

use self::{
    ad_request::{AdPlacementRequest, AdRequest},
    ad_response::{AdResponse, AdResponseValue},
    error::{
        CallbackRequestError, FetchAdsError, RecordClickError, RecordImpressionError, ReportAdError,
    },
    preflight::PreflightRequest,
    transport::MARSTransport,
};
use crate::{
    http_cache::{HttpCache, RequestHash},
    telemetry::Telemetry,
    CachePolicy,
};
use url::Url;
use viaduct::{Headers, Request};

pub struct MARSClient<T>
where
    T: Clone + Telemetry,
{
    environment: Environment,
    telemetry: T,
    transport: MARSTransport<T>,
}

impl<T> MARSClient<T>
where
    T: Clone + Telemetry,
{
    pub fn new(environment: Environment, http_cache: Option<HttpCache>, telemetry: T) -> Self {
        let transport = MARSTransport::new(http_cache, telemetry.clone());
        Self {
            environment,
            telemetry,
            transport,
        }
    }

    pub fn clear_cache(&self) -> Result<(), rusqlite::Error> {
        self.transport.clear_cache()
    }

    pub fn fetch_ads<A>(
        &self,
        context_id: String,
        placements: Vec<AdPlacementRequest>,
        cache_policy: CachePolicy,
        ohttp: bool,
    ) -> Result<(AdResponse<A>, RequestHash), FetchAdsError>
    where
        A: AdResponseValue,
    {
        let url = self.environment.into_url("ads");
        let mut ad_request = AdRequest::try_new(context_id, placements, url, ohttp)?;
        let request_hash = RequestHash::new(&ad_request);

        if ohttp {
            ad_request
                .headers
                .extend(Headers::from(self.fetch_preflight()?));
        }

        let response = self.transport.send(ad_request, &cache_policy, ohttp)?;
        let ads = AdResponse::<A>::parse(response.json()?, &self.telemetry)?;
        Ok((ads, request_hash))
    }

    // TODO: Remove this allow(dead_code) when cache invalidation is re-enabled behind Nimbus experiment
    #[allow(dead_code)]
    pub fn invalidate_cache_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<(), rusqlite::Error> {
        self.transport.invalidate_cache_by_hash(request_hash)
    }

    pub fn record_click(&self, callback: Url, ohttp: bool) -> Result<(), RecordClickError> {
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    pub fn record_impression(
        &self,
        callback: Url,
        ohttp: bool,
    ) -> Result<(), RecordImpressionError> {
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    pub fn report_ad(
        &self,
        mut callback: Url,
        reason: ReportReason,
        ohttp: bool,
    ) -> Result<(), ReportAdError> {
        callback
            .query_pairs_mut()
            .append_pair("reason", reason.as_str());
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    fn fetch_preflight(&self) -> Result<preflight::PreflightResponse, CallbackRequestError> {
        let response = self.transport.send(
            PreflightRequest(self.environment.into_url("ads-preflight")),
            &CachePolicy::CacheFirst { ttl: None },
            false,
        )?;
        Ok(response.json()?)
    }

    fn make_callback_request(
        &self,
        callback: Url,
        ohttp: bool,
    ) -> Result<(), CallbackRequestError> {
        let mut request = Request::get(callback);
        if ohttp {
            request
                .headers
                .extend(Headers::from(self.fetch_preflight()?));
        }
        self.transport.fire(request, ohttp).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {

    use super::ad_response::AdImage;
    use super::*;
    use crate::ffi::telemetry::MozAdsTelemetryWrapper;
    use crate::test_utils::{
        get_example_happy_image_response, make_happy_placement_requests, TEST_CONTEXT_ID,
    };
    use mockito::mock;

    fn make_test_client(http_cache: Option<HttpCache>) -> MARSClient<MozAdsTelemetryWrapper> {
        MARSClient::new(
            Environment::Test,
            http_cache,
            MozAdsTelemetryWrapper::noop(),
        )
    }

    #[test]
    fn test_record_impression_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/impression_callback_url")
            .with_status(200)
            .create();
        let client = make_test_client(None);
        let url = Url::parse(&format!(
            "{}/impression_callback_url",
            &mockito::server_url()
        ))
        .unwrap();
        let result = client.record_impression(url, false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_click_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/click_callback_url").with_status(200).create();

        let client = make_test_client(None);
        let url = Url::parse(&format!("{}/click_callback_url", &mockito::server_url())).unwrap();
        let result = client.record_click(url, false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_report_ad_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/report_ad_callback_url")
            .match_query(mockito::Matcher::UrlEncoded(
                "reason".into(),
                "not_interested".into(),
            ))
            .with_status(200)
            .create();

        let client = make_test_client(None);
        let url = Url::parse(&format!(
            "{}/report_ad_callback_url",
            &mockito::server_url()
        ))
        .unwrap();
        let result = client.report_ad(url, ReportReason::NotInterested, false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_fetch_ads_success() {
        viaduct_dev::init_backend_dev();
        let expected_response = get_example_happy_image_response();

        let m = mock("POST", "/ads")
            .match_header("content-type", "application/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let client = make_test_client(None);

        let result = client.fetch_ads::<AdImage>(
            TEST_CONTEXT_ID.to_string(),
            make_happy_placement_requests(),
            CachePolicy::default(),
            false,
        );
        assert!(result.is_ok());
        let (response, _request_hash) = result.unwrap();
        assert_eq!(expected_response, response);
        m.assert();
    }

    #[test]
    fn test_fetch_ads_cache_hit_skips_network() {
        viaduct_dev::init_backend_dev();
        let expected = get_example_happy_image_response();
        let m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected.data).unwrap())
            .expect(1) // only first request goes to network
            .create();

        let cache = HttpCache::builder("test_fetch_ads_cache_hit_skips_network.db")
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::http_cache::ByteSize::mib(1))
            .build()
            .unwrap();
        let client = make_test_client(Some(cache));

        // First call should be a miss then warm the cache
        let (response1, _) = client
            .fetch_ads::<AdImage>(
                TEST_CONTEXT_ID.to_string(),
                make_happy_placement_requests(),
                CachePolicy::default(),
                false,
            )
            .unwrap();
        assert_eq!(response1, expected);

        // Second call should be a hit
        let (response2, _) = client
            .fetch_ads::<AdImage>(
                TEST_CONTEXT_ID.to_string(),
                make_happy_placement_requests(),
                CachePolicy::default(),
                false,
            )
            .unwrap();
        assert_eq!(response2, expected);
        m.assert();
    }

    #[test]
    fn test_record_click_makes_callback_request() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_click.db")
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::http_cache::ByteSize::mib(1))
            .build()
            .unwrap();

        let client = make_test_client(Some(cache));
        let callback_url = Url::parse(&format!("{}/click", mockito::server_url())).unwrap();

        let m = mock("GET", "/click").with_status(200).create();

        let result = client.record_click(callback_url, false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_impression_makes_callback_request() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_impression.db")
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::http_cache::ByteSize::mib(1))
            .build()
            .unwrap();

        let client = make_test_client(Some(cache));
        let callback_url = Url::parse(&format!("{}/impression", mockito::server_url())).unwrap();

        let m = mock("GET", "/impression").with_status(200).create();

        let result = client.record_impression(callback_url, false);
        assert!(result.is_ok());
        m.assert();
    }
}
