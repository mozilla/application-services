/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub(crate) mod ad_request;
pub(crate) mod ad_response;
pub(crate) mod environment;
pub(crate) mod error;
mod preflight;
pub(crate) mod report_reason;
mod transport;

pub(crate) use environment::Environment;
use parking_lot::RwLock;
pub(crate) use report_reason::ReportReason;
use sql_support::open_database;

use self::{
    ad_request::{AdPlacementRequest, AdRequest, AdRequestFlags, AdRequestRelevance},
    ad_response::AdResponse,
    error::{
        CallbackRequestError, FetchAdsError, RecordClickError, RecordImpressionError, ReportAdError,
    },
    preflight::PreflightRequest,
    transport::MARSTransport,
};
use crate::{
    http_cache::{HttpCache, RequestHash},
    CachePolicy,
};
use url::Url;
use viaduct::{Headers, Request};

pub(crate) struct MARSClient {
    environment: Environment,
    transport: MARSTransport,
    request_options: RwLock<MARSRequestOptions>,
}

impl MARSClient {
    pub(crate) fn new(
        environment: Environment,
        http_cache: Option<HttpCache>,
        request_options: MARSRequestOptions,
    ) -> Self {
        Self {
            environment,
            transport: MARSTransport::new(http_cache),
            request_options: RwLock::new(request_options),
        }
    }

    pub fn set_request_options(&self, new_options: MARSRequestOptions) {
        let mut request_options = self.request_options.write();
        *request_options = new_options;
    }

    pub(crate) fn clear_cache(&self) -> Result<(), open_database::Error> {
        self.transport.clear_cache()
    }

    pub(crate) fn fetch_ads(
        &self,
        context_id: Option<String>,
        request_options: Option<MARSRequestOptions>,
        placements: Vec<AdPlacementRequest>,
        relevance: AdRequestRelevance,
    ) -> Result<(AdResponse, RequestHash), FetchAdsError> {
        // Get a temporary lock on settings- enough to clone.
        let request_options = request_options.unwrap_or(self.request_options.read().clone());
        let mut ad_request = AdRequest::try_new(
            request_options.blocks,
            context_id,
            self.environment.clone(),
            request_options.flags,
            request_options.ohttp,
            placements,
            relevance,
        )?;
        let request_hash = RequestHash::new(&ad_request);

        if request_options.ohttp {
            ad_request
                .headers
                .extend(Headers::try_from(self.fetch_preflight()?)?);
        }

        let response = self.transport.send(
            ad_request,
            &request_options.cache_policy,
            request_options.ohttp,
        )?;
        let ads: AdResponse = response.json()?;
        Ok((ads, request_hash))
    }

    #[expect(
        dead_code,
        reason = "cache invalidation on click is off until a Nimbus experiment re-enables it"
    )]
    pub(crate) fn invalidate_cache_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<(), open_database::Error> {
        self.transport.invalidate_cache_by_hash(request_hash)
    }

    pub(crate) fn record_click(&self, callback: Url, ohttp: bool) -> Result<(), RecordClickError> {
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    pub(crate) fn record_impression(
        &self,
        callback: Url,
        ohttp: bool,
    ) -> Result<(), RecordImpressionError> {
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    pub(crate) fn report_ad(
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

    pub(crate) fn shutdown_db(&self) {
        self.transport.shutdown_db();
    }

    fn fetch_preflight(&self) -> Result<preflight::PreflightResponse, CallbackRequestError> {
        let response = self.transport.send(
            PreflightRequest(self.environment.clone().into_url("ads-preflight")),
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
                .extend(Headers::try_from(self.fetch_preflight()?)?);
        }
        self.transport.fire(request, ohttp).map_err(Into::into)
    }
}

#[derive(Clone, Default)]
pub struct MARSRequestOptions {
    pub flags: AdRequestFlags,
    pub cache_policy: CachePolicy,
    pub ohttp: bool,
    pub blocks: Vec<String>,
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::test_utils::{
        get_example_happy_image_response, make_happy_placement_requests, TEST_CONTEXT_ID,
    };
    use mockito::mock;

    fn make_test_client(http_cache: Option<HttpCache>) -> MARSClient {
        MARSClient::new(Environment::Test, http_cache, MARSRequestOptions::default())
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

        let result = client.fetch_ads(
            Some(TEST_CONTEXT_ID.to_string()),
            None,
            make_happy_placement_requests(),
            AdRequestRelevance::default(),
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

        let cache = HttpCache::builder_memory()
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
            .build()
            .unwrap();
        let client = make_test_client(Some(cache));

        // First call should be a miss then warm the cache
        let (response1, _) = client
            .fetch_ads(
                Some(TEST_CONTEXT_ID.to_string()),
                None,
                make_happy_placement_requests(),
                AdRequestRelevance::default(),
            )
            .unwrap();
        assert_eq!(response1, expected);

        // Second call should be a hit
        let (response2, _) = client
            .fetch_ads(
                Some(TEST_CONTEXT_ID.to_string()),
                None,
                make_happy_placement_requests(),
                AdRequestRelevance::default(),
            )
            .unwrap();
        assert_eq!(response2, expected);
        m.assert();
    }

    #[test]
    fn test_record_click_makes_callback_request() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder_memory()
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
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
        let cache = HttpCache::builder_memory()
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
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
