/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub(crate) mod ad_request;
pub(crate) mod ad_response;
mod delete_user;
pub(crate) mod environment;
pub(crate) mod error;
mod preflight;
pub(crate) mod report_reason;
mod transport;

pub(crate) use environment::Environment;
pub(crate) use report_reason::ReportReason;
use sql_support::open_database;

use self::{
    ad_request::{AdPlacementRequest, AdRequest, AdRequestFlags, AdRequestRelevance},
    ad_response::AdResponse,
    delete_user::DeleteUserRequest,
    error::{
        CallbackRequestError, DeleteUserError, FetchAdsError, RecordClickError,
        RecordImpressionError, ReportAdError,
    },
    preflight::PreflightRequest,
    transport::MARSTransport,
};
use crate::{
    http_cache::{HttpCache, RequestHash},
    non_empty_string::NonEmptyString,
    CachePolicy,
};
use url::Url;
use viaduct::{Headers, Request};

/// Timeout for the best-effort deletion request, which runs inside an ad request.
/// Over OHTTP it bounds the relay request; the gateway key fetch keeps its own.
const DELETE_USER_TIMEOUT_MS: u32 = 5_000;

pub(crate) struct MARSClient {
    environment: Environment,
    transport: MARSTransport,
    user_agent: Option<NonEmptyString>,
}

impl MARSClient {
    pub(crate) fn new(
        environment: Environment,
        http_cache: Option<HttpCache>,
        user_agent: Option<NonEmptyString>,
    ) -> Self {
        Self {
            environment,
            transport: MARSTransport::new(http_cache),
            user_agent,
        }
    }

    pub(crate) fn clear_cache(&self) -> Result<(), open_database::Error> {
        self.transport.clear_cache()
    }

    /// Sends `DELETE /delete_user` for `context_id`, over OHTTP when `ohttp` is set.
    pub(crate) fn delete_user(&self, context_id: &str, ohttp: bool) -> Result<(), DeleteUserError> {
        let request = Request::delete(self.environment.clone().into_url("delete_user"))
            .json(&DeleteUserRequest { context_id });
        self.transport
            .fire_with_timeout(request, ohttp, DELETE_USER_TIMEOUT_MS)
            .map_err(Into::into)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the fields of an ad request, and how to send it"
    )]
    pub(crate) fn fetch_ads(
        &self,
        context_id: Option<String>,
        flags: AdRequestFlags,
        placements: Vec<AdPlacementRequest>,
        cache_policy: CachePolicy,
        ohttp: bool,
        blocks: Vec<String>,
        relevance: AdRequestRelevance,
    ) -> Result<(AdResponse, RequestHash), FetchAdsError> {
        let mut ad_request = AdRequest::try_new(
            blocks,
            context_id,
            self.environment.clone(),
            flags,
            ohttp,
            placements,
            relevance,
        )?;
        let request_hash = RequestHash::new(&ad_request);

        if ohttp {
            ad_request.headers.extend(self.preflight_headers()?);
        }

        let response = self.transport.send(ad_request, &cache_policy, ohttp)?;
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

    fn preflight_headers(&self) -> Result<Headers, CallbackRequestError> {
        self.fetch_preflight()?
            .into_headers(self.user_agent.as_ref())
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
            request.headers.extend(self.preflight_headers()?);
        }
        self.transport.fire(request, ohttp).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::test_utils::{
        get_example_happy_image_response, make_happy_placement_requests, TEST_CONTEXT_ID,
    };
    use mockito::mock;

    fn make_test_client(http_cache: Option<HttpCache>) -> MARSClient {
        MARSClient::new(Environment::Test, http_cache, None)
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
            AdRequestFlags::default(),
            make_happy_placement_requests(),
            CachePolicy::default(),
            false,
            Default::default(),
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

        let cache = HttpCache::builder()
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
            .build()
            .unwrap();
        let client = make_test_client(Some(cache));

        // First call should be a miss then warm the cache
        let (response1, _) = client
            .fetch_ads(
                Some(TEST_CONTEXT_ID.to_string()),
                AdRequestFlags::default(),
                make_happy_placement_requests(),
                CachePolicy::default(),
                false,
                Default::default(),
                AdRequestRelevance::default(),
            )
            .unwrap();
        assert_eq!(response1, expected);

        // Second call should be a hit
        let (response2, _) = client
            .fetch_ads(
                Some(TEST_CONTEXT_ID.to_string()),
                AdRequestFlags::default(),
                make_happy_placement_requests(),
                CachePolicy::default(),
                false,
                Default::default(),
                AdRequestRelevance::default(),
            )
            .unwrap();
        assert_eq!(response2, expected);
        m.assert();
    }

    #[test]
    fn test_record_click_makes_callback_request() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder()
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
        let cache = HttpCache::builder()
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

    #[test]
    fn test_delete_user_sends_json_delete() {
        viaduct_dev::init_backend_dev();
        let m = mock("DELETE", "/delete_user")
            .match_header("content-type", "application/json")
            .match_body(mockito::Matcher::Json(
                serde_json::json!({ "context_id": TEST_CONTEXT_ID }),
            ))
            .with_status(200)
            .expect(1)
            .create();

        let client = make_test_client(None);
        let result = client.delete_user(TEST_CONTEXT_ID, false);

        assert!(result.is_ok(), "unexpected error: {result:?}");
        m.assert();
    }

    #[test]
    fn test_delete_user_without_ohttp_channel_fails_closed() {
        viaduct_dev::init_backend_dev();
        // No channel configured: fail closed, never fall back to a direct DELETE.
        let m = mock("DELETE", "/delete_user").expect(0).create();

        let client = make_test_client(None);
        let result = client.delete_user(TEST_CONTEXT_ID, true);

        assert!(matches!(
            result,
            Err(DeleteUserError::Request(
                viaduct::ViaductError::OhttpChannelNotConfigured(ref channel)
            )) if channel == "ads-client"
        ));
        m.assert();
    }
}
