/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use context_id::{ContextIDComponent, DefaultContextIdCallback};
use once_cell::sync::Lazy;
use url::Url;
use viaduct::Request;

use crate::{
    client::{ad_request::AdRequest, ad_response::AdResponse, config::Environment},
    error::{
        check_http_status_for_error, CallbackRequestError, FetchAdsError, RecordClickError,
        RecordImpressionError, ReportAdError,
    },
    http_cache::{CacheOutcome, HttpCache, HttpCacheError},
    RequestCachePolicy,
};

static MARS_API_ENDPOINT_PROD: Lazy<Url> =
    Lazy::new(|| Url::parse("https://ads.mozilla.org/v1/").expect("hardcoded URL must be valid"));

#[cfg(feature = "dev")]
static MARS_API_ENDPOINT_STAGING: Lazy<Url> =
    Lazy::new(|| Url::parse("https://ads.allizom.org/v1/").expect("hardcoded URL must be valid"));

impl Environment {
    pub fn into_mars_url(self) -> &'static Url {
        match self {
            Environment::Prod => &MARS_API_ENDPOINT_PROD,
            #[cfg(feature = "dev")]
            Environment::Staging => &MARS_API_ENDPOINT_STAGING,
        }
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait MARSClient: Sync + Send {
    fn fetch_ads(
        &self,
        request: &AdRequest,
        cache_policy: &RequestCachePolicy,
    ) -> Result<AdResponse, FetchAdsError>;
    fn record_impression(&self, callback: Url) -> Result<(), RecordImpressionError>;
    fn record_click(&self, callback: Url) -> Result<(), RecordClickError>;
    fn report_ad(&self, callback: Url) -> Result<(), ReportAdError>;
    fn get_context_id(&self) -> context_id::ApiResult<String>;
    fn cycle_context_id(&mut self) -> context_id::ApiResult<String>;
    fn get_mars_endpoint(&self) -> &Url;
    fn clear_cache(&self) -> Result<(), HttpCacheError>;
}

pub struct DefaultMARSClient {
    context_id_component: ContextIDComponent,
    endpoint: Url,
    http_cache: Option<HttpCache>,
}

impl DefaultMARSClient {
    pub fn new(
        context_id: String,
        environment: Environment,
        http_cache: Option<HttpCache>,
    ) -> Self {
        let endpoint = environment.into_mars_url().clone();

        Self {
            context_id_component: ContextIDComponent::new(
                &context_id,
                0,
                false,
                Box::new(DefaultContextIdCallback),
            ),
            endpoint,
            http_cache,
        }
    }

    #[cfg(test)]
    pub fn new_with_endpoint(
        context_id: String,
        endpoint: String,
        http_cache: Option<HttpCache>,
    ) -> Self {
        Self {
            context_id_component: ContextIDComponent::new(
                &context_id,
                0,
                false,
                Box::new(DefaultContextIdCallback),
            ),
            endpoint: Url::parse(endpoint.as_str()).unwrap(),
            http_cache,
        }
    }

    fn make_callback_request(&self, callback: Url) -> Result<(), CallbackRequestError> {
        let request = Request::get(callback);
        let response = request.send()?;
        check_http_status_for_error(&response).map_err(Into::into)
    }
}

impl MARSClient for DefaultMARSClient {
    fn cycle_context_id(&mut self) -> context_id::ApiResult<String> {
        let old_context_id = self.get_context_id()?;
        self.context_id_component.force_rotation()?;
        Ok(old_context_id)
    }

    fn get_context_id(&self) -> context_id::ApiResult<String> {
        self.context_id_component.request(0)
    }

    fn get_mars_endpoint(&self) -> &Url {
        &self.endpoint
    }

    fn fetch_ads(
        &self,
        ad_request: &AdRequest,
        cache_policy: &RequestCachePolicy,
    ) -> Result<AdResponse, FetchAdsError> {
        let base = self.get_mars_endpoint();
        let url = base.join("ads")?;
        let request = Request::post(url).json(ad_request);

        if let Some(cache) = self.http_cache.as_ref() {
            let outcome = cache.send_with_policy(&request, cache_policy)?;

            // TODO: observe cache outcome for metrics/logging.
            match &outcome.cache_outcome {
                CacheOutcome::Hit => {}
                CacheOutcome::LookupFailed(_err) => {}
                CacheOutcome::NoCache => {}
                CacheOutcome::MissNotCacheable => {}
                CacheOutcome::MissStored => {}
                CacheOutcome::StoreFailed(_err) => {}
                CacheOutcome::CleanupFailed(_err) => {}
            }
            check_http_status_for_error(&outcome.response)?;
            let response_json: AdResponse = outcome.response.json()?;
            Ok(response_json)
        } else {
            let response = request.send()?;
            check_http_status_for_error(&response)?;
            let response_json: AdResponse = response.json()?;
            Ok(response_json)
        }
    }

    fn record_impression(&self, callback: Url) -> Result<(), RecordImpressionError> {
        Ok(self.make_callback_request(callback)?)
    }

    fn record_click(&self, callback: Url) -> Result<(), RecordClickError> {
        Ok(self.make_callback_request(callback)?)
    }

    fn report_ad(&self, callback: Url) -> Result<(), ReportAdError> {
        Ok(self.make_callback_request(callback)?)
    }

    fn clear_cache(&self) -> Result<(), HttpCacheError> {
        if let Some(cache) = &self.http_cache {
            cache.clear()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use mockito::mock;
    use url::Host;

    use super::*;
    use crate::test_utils::{
        create_test_client, get_example_happy_ad_response, make_happy_ad_request, TEST_CONTEXT_ID,
    };

    #[test]
    fn test_get_context_id() {
        let client = create_test_client(mockito::server_url());
        assert_eq!(
            client.get_context_id().unwrap(),
            TEST_CONTEXT_ID.to_string()
        );
    }

    #[test]
    fn test_cycle_context_id() {
        let mut client = create_test_client(mockito::server_url());
        let old_id = client.cycle_context_id().unwrap();
        assert_eq!(old_id, TEST_CONTEXT_ID);
        assert_ne!(client.get_context_id().unwrap(), TEST_CONTEXT_ID);
    }

    #[test]
    fn test_record_impression_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let _m = mock("GET", "/impression_callback_url")
            .with_status(200)
            .create();
        let client = create_test_client(mockito::server_url());
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

        let client = create_test_client(mockito::server_url());
        let url = Url::parse(&format!("{}/click_callback_url", &mockito::server_url())).unwrap();
        let result = client.record_click(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_report_ad_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let _m = mock("GET", "/report_ad_callback_url")
            .with_status(200)
            .create();

        let client = create_test_client(mockito::server_url());
        let url = Url::parse(&format!(
            "{}/report_ad_callback_url",
            &mockito::server_url()
        ))
        .unwrap();
        let result = client.report_ad(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fetch_ads_success() {
        viaduct_dev::init_backend_dev();
        let expected_response = get_example_happy_ad_response();

        let _m = mock("POST", "/ads")
            .match_header("content-type", "application/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response).unwrap())
            .create();

        let client = create_test_client(mockito::server_url());

        let ad_request = make_happy_ad_request();

        let result = client.fetch_ads(&ad_request, &RequestCachePolicy::default());
        assert!(result.is_ok());
        assert_eq!(expected_response, result.unwrap());
    }

    #[test]
    fn test_fetch_ads_cache_hit_skips_network() {
        viaduct_dev::init_backend_dev();
        let expected = get_example_happy_ad_response();
        let _m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected).unwrap())
            .expect(1) // only first request goes to network
            .create();

        let client = create_test_client(mockito::server_url());
        let ad_request = make_happy_ad_request();

        // First call should be a miss then warm the cache
        assert_eq!(
            client
                .fetch_ads(&ad_request, &RequestCachePolicy::default())
                .unwrap(),
            expected
        );
        // Second call should be a hit
        assert_eq!(
            client
                .fetch_ads(&ad_request, &RequestCachePolicy::default())
                .unwrap(),
            expected
        );
    }

    #[test]
    fn prod_endpoint_parses_and_is_expected() {
        let url = Environment::Prod.into_mars_url();

        assert_eq!(url.as_str(), "https://ads.mozilla.org/v1/");

        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host(), Some(Host::Domain("ads.mozilla.org")));
        assert_eq!(url.path(), "/v1/");

        let url2 = Environment::Prod.into_mars_url();
        assert!(std::ptr::eq(url, url2));
    }

    #[cfg(feature = "dev")]
    #[test]
    fn staging_endpoint_parses_and_is_expected() {
        let url = Environment::Staging.into_mars_url();

        assert_eq!(url.as_str(), "https://ads.allizom.org/v1/");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.domain(), Some("ads.allizom.org"));
        assert_eq!(url.path(), "/v1/");

        let url2 = Environment::Staging.into_mars_url();
        assert!(std::ptr::eq(url, url2));
    }

    #[test]
    fn default_client_uses_prod_url() {
        let client = DefaultMARSClient::new("ctx".into(), Environment::Prod, None);
        assert_eq!(
            client.get_mars_endpoint().as_str(),
            "https://ads.mozilla.org/v1/"
        );
    }
}
