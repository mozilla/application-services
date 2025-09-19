/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::{
    error::{
        check_http_status_for_error, CallbackRequestError, FetchAdsError, RecordClickError,
        RecordImpressionError, ReportAdError,
    },
    models::{AdRequest, AdResponse},
};
use context_id::{ContextIDComponent, DefaultContextIdCallback};
use url::Url;
use viaduct::Request;

const DEFAULT_MARS_API_ENDPOINT: &str = "https://ads.mozilla.org/v1";

#[cfg_attr(test, mockall::automock)]
pub trait MARSClient: Sync + Send {
    fn fetch_ads(&self, request: &AdRequest) -> Result<AdResponse, FetchAdsError>;
    fn record_impression(
        &self,
        url_callback_string: Option<String>,
    ) -> Result<(), RecordImpressionError>;
    fn record_click(&self, url_callback_string: Option<String>) -> Result<(), RecordClickError>;
    fn report_ad(&self, url_callback_string: Option<String>) -> Result<(), ReportAdError>;
    fn get_context_id(&self) -> context_id::ApiResult<String>;
    fn cycle_context_id(&mut self) -> context_id::ApiResult<String>;
    fn get_mars_endpoint(&self) -> &str;
}

pub struct DefaultMARSClient {
    context_id_component: ContextIDComponent,
    endpoint: String,
}

impl DefaultMARSClient {
    pub fn new(context_id: String) -> Self {
        Self {
            context_id_component: ContextIDComponent::new(
                &context_id,
                0,
                false,
                Box::new(DefaultContextIdCallback),
            ),
            endpoint: DEFAULT_MARS_API_ENDPOINT.to_string(),
        }
    }

    #[cfg(test)]
    pub fn new_with_endpoint(context_id: String, endpoint: String) -> Self {
        Self {
            context_id_component: ContextIDComponent::new(
                &context_id,
                0,
                false,
                Box::new(DefaultContextIdCallback),
            ),
            endpoint,
        }
    }

    fn make_callback_request(&self, url_callback_string: &str) -> Result<(), CallbackRequestError> {
        let url = Url::parse(url_callback_string)?;
        let request = Request::get(url);
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

    fn get_mars_endpoint(&self) -> &str {
        &self.endpoint
    }

    fn fetch_ads(&self, ad_request: &AdRequest) -> Result<AdResponse, FetchAdsError> {
        let endpoint = self.get_mars_endpoint();
        let url = Url::parse(&format!("{endpoint}/ads"))?;
        let request = Request::post(url).json(ad_request);
        let response = request.send()?;

        check_http_status_for_error(&response)?;

        let response_json: AdResponse = response.json()?;

        Ok(response_json)
    }

    fn record_impression(
        &self,
        url_callback_string: Option<String>,
    ) -> Result<(), RecordImpressionError> {
        match url_callback_string {
            Some(callback) => self.make_callback_request(&callback).map_err(Into::into),
            None => Err(CallbackRequestError::MissingCallback {
                message: "Impression callback url empty.".to_string(),
            }
            .into()),
        }
    }

    fn record_click(&self, url_callback_string: Option<String>) -> Result<(), RecordClickError> {
        match url_callback_string {
            Some(callback) => self.make_callback_request(&callback).map_err(Into::into),
            None => Err(CallbackRequestError::MissingCallback {
                message: "Click callback url empty.".to_string(),
            }
            .into()),
        }
    }

    fn report_ad(&self, url_callback_string: Option<String>) -> Result<(), ReportAdError> {
        match url_callback_string {
            Some(callback) => self.make_callback_request(&callback).map_err(Into::into),
            None => Err(CallbackRequestError::MissingCallback {
                message: "Report callback url empty.".to_string(),
            }
            .into()),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        models::AdPlacementRequest,
        test_utils::{create_test_client, get_example_happy_ad_response, TEST_CONTEXT_ID},
    };
    use mockito::mock;

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
    fn test_record_impression_with_empty_callback_should_fail() {
        let client = create_test_client(mockito::server_url());
        let result = client.record_impression(None);
        assert!(result.is_err());
    }

    #[test]
    fn test_record_click_with_empty_callback_should_fail() {
        let client = create_test_client(mockito::server_url());
        let result = client.record_click(None);
        assert!(result.is_err());
    }

    #[test]
    fn test_record_report_with_empty_callback_should_fail() {
        let client = create_test_client(mockito::server_url());
        let result = client.report_ad(None);
        assert!(result.is_err());
    }

    #[test]
    fn test_record_impression_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let _m = mock("GET", "/impression_callback_url")
            .with_status(200)
            .create();
        let client = create_test_client(mockito::server_url());
        let url = format!("{}/impression_callback_url", &mockito::server_url());
        let result = client.record_impression(Some(url));
        assert!(result.is_ok());
    }

    #[test]
    fn test_record_click_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let _m = mock("GET", "/click_callback_url").with_status(200).create();

        let client = create_test_client(mockito::server_url());
        let url = format!("{}/click_callback_url", &mockito::server_url());
        let result = client.record_click(Some(url));
        assert!(result.is_ok());
    }

    #[test]
    fn test_report_ad_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let _m = mock("GET", "/report_ad_callback_url")
            .with_status(200)
            .create();

        let client = create_test_client(mockito::server_url());
        let url = format!("{}/report_ad_callback_url", &mockito::server_url());
        let result = client.report_ad(Some(url));
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

        let ad_request = AdRequest {
            context_id: client.get_context_id().unwrap().to_string(),
            placements: vec![
                AdPlacementRequest {
                    placement: "example_placement_1".to_string(),
                    count: 1,
                    content: None,
                },
                AdPlacementRequest {
                    placement: "example_placement_2".to_string(),
                    count: 1,
                    content: None,
                },
            ],
        };

        let result = client.fetch_ads(&ad_request);
        assert!(result.is_ok());
        assert_eq!(expected_response, result.unwrap());
    }
}
