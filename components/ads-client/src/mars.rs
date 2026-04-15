/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod ad_request;
pub mod ad_response;
pub mod environment;
pub mod error;
pub mod report_reason;

pub use environment::Environment;
pub use report_reason::ReportReason;

use self::{
    ad_request::{AdPlacementRequest, AdRequest},
    ad_response::{AdResponse, AdResponseValue},
    error::{
        check_http_status_for_error, CallbackRequestError, FetchAdsError, RecordClickError,
        RecordImpressionError, ReportAdError,
    },
};
use crate::{
    http_cache::{HttpCache, RequestHash},
    telemetry::Telemetry,
    CachePolicy,
};

use url::Url;
use viaduct::{Client, ClientSettings, Request};

pub struct MARSClient<T>
where
    T: Telemetry,
{
    environment: Environment,
    http_cache: Option<HttpCache>,
    telemetry: T,
}

impl<T> MARSClient<T>
where
    T: Telemetry,
{
    pub fn new(environment: Environment, http_cache: Option<HttpCache>, telemetry: T) -> Self {
        Self {
            environment,
            http_cache,
            telemetry,
        }
    }

    pub fn clear_cache(&self) -> Result<(), rusqlite::Error> {
        if let Some(cache) = &self.http_cache {
            cache.clear()?;
        }
        Ok(())
    }

    pub fn fetch_ads<A>(
        &self,
        context_id: String,
        placements: Vec<AdPlacementRequest>,
        cache_policy: CachePolicy,
    ) -> Result<(AdResponse<A>, RequestHash), FetchAdsError>
    where
        A: AdResponseValue,
    {
        let url = self.environment.into_url("ads");
        let ad_request = AdRequest::try_new(context_id, placements, url)?;
        let request_hash = RequestHash::new(&ad_request);

        let client = self.client_for();
        let response: AdResponse<A> = if let Some(cache) = self.http_cache.as_ref() {
            let (response, cache_outcomes) =
                cache.send_with_policy(&client, ad_request, &cache_policy)?;
            for outcome in &cache_outcomes {
                self.telemetry.record(outcome);
            }
            check_http_status_for_error(&response)?;
            AdResponse::<A>::parse(response.json()?, &self.telemetry)?
        } else {
            let request: Request = ad_request.into();
            let response = client.send_sync(request)?;
            check_http_status_for_error(&response)?;
            AdResponse::<A>::parse(response.json()?, &self.telemetry)?
        };
        Ok((response, request_hash))
    }

    // TODO: Remove this allow(dead_code) when cache invalidation is re-enabled behind Nimbus experiment
    #[allow(dead_code)]
    pub fn invalidate_cache_by_hash(
        &self,
        request_hash: &crate::http_cache::RequestHash,
    ) -> Result<(), rusqlite::Error> {
        if let Some(cache) = &self.http_cache {
            cache.invalidate_by_hash(request_hash)?;
        }
        Ok(())
    }

    pub fn record_click(&self, callback: Url) -> Result<(), RecordClickError> {
        Ok(self.make_callback_request(callback)?)
    }

    pub fn record_impression(&self, callback: Url) -> Result<(), RecordImpressionError> {
        Ok(self.make_callback_request(callback)?)
    }

    pub fn report_ad(&self, mut callback: Url, reason: ReportReason) -> Result<(), ReportAdError> {
        callback
            .query_pairs_mut()
            .append_pair("reason", reason.as_str());
        Ok(self.make_callback_request(callback)?)
    }

    fn client_for(&self) -> Client {
        Client::new(ClientSettings::default())
    }

    fn make_callback_request(&self, callback: Url) -> Result<(), CallbackRequestError> {
        let client = Client::new(ClientSettings::default());
        let request = Request::get(callback);
        let response = client.send_sync(request)?;
        check_http_status_for_error(&response).map_err(Into::into)
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
        let _m = mock("GET", "/impression_callback_url")
            .with_status(200)
            .create();
        let client = make_test_client(None);
        let url = Url::parse(&format!(
            "{}/impression_callback_url",
            &mockito::server_url()
        ))
        .unwrap();
        let result = client.record_impression(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_record_click_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let _m = mock("GET", "/click_callback_url").with_status(200).create();

        let client = make_test_client(None);
        let url = Url::parse(&format!("{}/click_callback_url", &mockito::server_url())).unwrap();
        let result = client.record_click(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_report_ad_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let _m = mock("GET", "/report_ad_callback_url")
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
        let result = client.report_ad(url, ReportReason::NotInterested);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fetch_ads_success() {
        viaduct_dev::init_backend_dev();
        let expected_response = get_example_happy_image_response();

        let _m = mock("POST", "/ads")
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
        );
        assert!(result.is_ok());
        let (response, _request_hash) = result.unwrap();
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_fetch_ads_cache_hit_skips_network() {
        viaduct_dev::init_backend_dev();
        let expected = get_example_happy_image_response();
        let _m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected.data).unwrap())
            .expect(1) // only first request goes to network
            .create();

        let client = make_test_client(None);

        // First call should be a miss then warm the cache
        let (response1, _) = client
            .fetch_ads::<AdImage>(
                TEST_CONTEXT_ID.to_string(),
                make_happy_placement_requests(),
                CachePolicy::default(),
            )
            .unwrap();
        assert_eq!(response1, expected);

        // Second call should be a hit
        let (response2, _) = client
            .fetch_ads::<AdImage>(
                TEST_CONTEXT_ID.to_string(),
                make_happy_placement_requests(),
                CachePolicy::default(),
            )
            .unwrap();
        assert_eq!(response2, expected);
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

        let _m = mock("GET", "/click").with_status(200).create();

        let result = client.record_click(callback_url);
        assert!(result.is_ok());
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

        let _m = mock("GET", "/impression").with_status(200).create();

        let result = client.record_impression(callback_url);
        assert!(result.is_ok());
    }
}
