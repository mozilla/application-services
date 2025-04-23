/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

mod error;
mod http;
mod models;

pub use error::{ApiResult, Error, Result};
use error_support::handle_error;
pub use models::{
    CuratedRecommendationsConfig, CuratedRecommendationsRequest, CuratedRecommendationsResponse,
};
use url::Url;

const DEFAULT_BASE_HOST: &str = "https://merino.services.mozilla.com";

#[derive(uniffi::Object)]
pub struct CuratedRecommendationsClient {
    inner: CuratedRecommendationsClientInner<http::HttpClient>,
    endpoint_url: Url,
    user_agent_header: String,
}

struct CuratedRecommendationsClientInner<T: http::HttpClientTrait> {
    http_client: T,
}

#[derive(Default)]
pub struct CuratedRecommendationsClientBuilder {
    base_host: Option<String>,
    user_agent_header: Option<String>,
}

impl CuratedRecommendationsClientBuilder {
    pub fn new() -> Self {
        Self {
            base_host: None,
            user_agent_header: None,
        }
    }

    pub fn base_host(mut self, base_host: impl Into<String>) -> Self {
        self.base_host = Some(base_host.into());
        self
    }

    pub fn user_agent_header(mut self, user_agent_header: impl Into<String>) -> Self {
        self.user_agent_header = Some(user_agent_header.into());
        self
    }

    pub fn build(self) -> Result<CuratedRecommendationsClient> {
        let user_agent_header = self.user_agent_header.ok_or_else(|| Error::Unexpected {
            code: 0,
            message: "user_agent_header must be provided".to_string(),
        })?;

        let base_host = self
            .base_host
            .unwrap_or_else(|| DEFAULT_BASE_HOST.to_string());

        let url = format!("{}/api/v1/curated-recommendations", base_host);
        let endpoint_url = Url::parse(&url)?;

        Ok(CuratedRecommendationsClient {
            inner: CuratedRecommendationsClientInner::new()?,
            endpoint_url,
            user_agent_header,
        })
    }
}

#[uniffi::export]
impl CuratedRecommendationsClient {
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(config: CuratedRecommendationsConfig) -> ApiResult<Self> {
        let mut builder =
            CuratedRecommendationsClientBuilder::new().user_agent_header(config.user_agent_header);

        if let Some(base_host) = config.base_host {
            builder = builder.base_host(base_host);
        }

        builder.build()
    }

    #[handle_error(Error)]
    pub fn get_curated_recommendations(
        &self,
        request: &CuratedRecommendationsRequest,
    ) -> ApiResult<CuratedRecommendationsResponse> {
        self.inner
            .get_curated_recommendations(request, &self.user_agent_header, &self.endpoint_url)
    }
}

impl CuratedRecommendationsClientInner<http::HttpClient> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http_client: http::HttpClient,
        })
    }
}

impl<T: http::HttpClientTrait> CuratedRecommendationsClientInner<T> {
    pub fn get_curated_recommendations(
        &self,
        request: &CuratedRecommendationsRequest,
        user_agent_header: &str,
        endpoint_url: &Url,
    ) -> Result<CuratedRecommendationsResponse> {
        self.http_client.make_curated_recommendation_request(
            request,
            user_agent_header,
            endpoint_url.clone(),
        )
    }
}

#[cfg(test)]
impl<T: http::HttpClientTrait> CuratedRecommendationsClientInner<T> {
    // allows us to inject a fake http client for testing
    pub fn new_with_client(client: T) -> Self {
        Self {
            http_client: client,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curated_recommendations::models::{
        CuratedRecommendationLocale, RecommendationDataItem, SectionSettings,
    };

    struct FakeHttpClientSuccess;

    impl http::HttpClientTrait for FakeHttpClientSuccess {
        fn make_curated_recommendation_request(
            &self,
            _request: &CuratedRecommendationsRequest,
            _user_agent_header: &str,
            _base_host: Url,
        ) -> Result<CuratedRecommendationsResponse> {
            Ok(CuratedRecommendationsResponse {
                recommended_at: 1740764371347,
                data: vec![
                    RecommendationDataItem {
                        corpus_item_id: "18fbf4e1-3a8b-4b28-84a0-b6b4b785a44b".to_string(),
                        scheduled_corpus_item_id: "af067d76-c72d-4dfa-ba53-7c6c5b204c17".to_string(),
                        url: "https://getpocket.com/explore/item/how-online-influencers-got-addicted-to-swedish-candy?utm_source=firefox-newtab-en-us".to_string(),
                        title: "How Online Influencers Got Addicted to Swedish Candy".to_string(),
                        excerpt: "TikTok’s obsession with Scandinavian sweets, which began in early 2024, has squeezed global supply chains and shows no signs of slowing down.".to_string(),
                        topic: Option::from("business".to_string()),
                        publisher: "Bloomberg Business week".to_string(),
                        is_time_sensitive: false,
                        image_url: "https://s3.us-east-1.amazonaws.com/pocket-curatedcorpusapi-prod-images/28e2f56b-40ae-407d-ac68-cae02e66434d.jpeg".to_string(),
                        icon_url: None,
                        tile_id: 6487013562368874,
                        received_rank: 0,
                    },
                    RecommendationDataItem {
                        corpus_item_id: "e215a2b8-b188-484b-af03-c0fd99f853f5".to_string(),
                        scheduled_corpus_item_id: "2de2453f-b38b-4713-8dcb-82660856efd7".to_string(),
                        url: "https://www.cbc.ca/news/us-canada-tariffs-housing-costs-1.7466822?utm_source=firefox-newtab-en-us".to_string(),
                        title: "The Threat of a Tariff War Is Already Driving up Housing Costs".to_string(),
                        excerpt: "Housing sector insiders say the mere threat of a tariff war with the U.S. is another painful blow to an industry that has been struggling to get projects off the ground and keep up with demand. ".to_string(),
                        topic: Option::from("business".to_string()),
                        publisher: "CBC".to_string(),
                        is_time_sensitive: false,
                        image_url: "https://i.cbc.ca/1.7334557.1727356371!/fileImage/httpImage/image.jpg_gen/derivatives/16x9_1180/housing-20240812.jpg?im=Resize%3D620".to_string(),
                        icon_url: Option::from("https://merino-images.services.mozilla.com/favicons/ccd270c8c839b5560cc10386689067bbdbcedc437c0f6f5caa2db3a4c69eb01c_4792.svg".to_string()),
                        tile_id: 2140274178832306,
                        received_rank: 1,
                    },
                    RecommendationDataItem {
                        corpus_item_id: "51bdde04-058f-458b-9c41-91c3cdea6d35".to_string(),
                        scheduled_corpus_item_id: "552466da-2345-4a8c-a714-8b45c4561031".to_string(),
                        url: "https://www.nbcnews.com/politics/trump-administration/states-brace-trump-plan-dismantle-education-department-rcna192953?utm_source=firefox-newtab-en-us".to_string(),
                        title: "‘We’re Not Prepared’: States Brace for Trump’s Plans to Dismantle the Education Department ".to_string(),
                        excerpt: "Trump has said he wants school policy to be left to the states, but state officials and lawmakers aren’t clear on what that would look like.  ".to_string(),
                        topic: Option::from("government".to_string()),
                        publisher: "NBC News".to_string(),
                        is_time_sensitive: true,
                        image_url: "https://s3.us-east-1.amazonaws.com/pocket-curatedcorpusapi-prod-images/4089978e-4a96-4b53-a715-5ca40079773d.jpeg".to_string(),
                        icon_url: Option::from("https://merino-images.services.mozilla.com/favicons/f72a169d901fe296f4cc35642ffc42d1c946bd56e81f9fa2fdbe0cf5ecdf1fc9_5052.png".to_string()),
                        tile_id: 4434955254511817,
                        received_rank: 2,
                    },
                    RecommendationDataItem {
                        corpus_item_id: "b0b2d1f0-312b-4d9a-9bda-ecd37f32fb40".to_string(),
                        scheduled_corpus_item_id: "37bf860b-c91a-4d99-9923-ba3b640502cf".to_string(),
                        url: "https://www.bbc.com/news/live/c625ex282zzt?utm_source=firefox-newtab-en-us".to_string(),
                        title: "Trump Tells Zelensky ‘Make a Deal or We’re Out’ in Angry White House Meeting".to_string(),
                        excerpt: "The US president calls his Ukrainian counterpart “disrespectful” and tells him to be “thankful” during heated exchanges in the Oval Office.".to_string(),
                        topic: Option::from("government".to_string()),
                        publisher: "BBC".to_string(),
                        is_time_sensitive: true,
                        image_url: "https://s3.us-east-1.amazonaws.com/pocket-curatedcorpusapi-prod-images/a7eeb1b7-e06a-4ce5-9259-d661176f3e43.png".to_string(),
                        icon_url: Option::from("https://merino-images.services.mozilla.com/favicons/388231ac048528715ffc2aebad84b58c19231d92156179d21c94d0b98d4f1d9b_751.svg".to_string()),
                        tile_id: 1271137535326463,
                        received_rank: 3,
                    }
                ],
                feeds: None,
                interest_picker: None
            })
        }
    }

    struct FakeHttpClientValidationError;

    impl http::HttpClientTrait for FakeHttpClientValidationError {
        fn make_curated_recommendation_request(
            &self,
            _request: &CuratedRecommendationsRequest,
            _user_agent_header: &str,
            _base_host: Url,
        ) -> Result<CuratedRecommendationsResponse> {
            Err(Error::Validation {
                code: 422,
                message: "Invalid input".to_string(),
            })
        }
    }

    struct FakeHttpClientServerError;

    impl http::HttpClientTrait for FakeHttpClientServerError {
        fn make_curated_recommendation_request(
            &self,
            _request: &CuratedRecommendationsRequest,
            _user_agent_header: &str,
            _base_host: Url,
        ) -> Result<CuratedRecommendationsResponse> {
            Err(Error::Server {
                code: 500,
                message: "The server encountered an unexpected error".to_string(),
            })
        }
    }

    struct FakeHttpClientBadRequestError;

    impl http::HttpClientTrait for FakeHttpClientBadRequestError {
        fn make_curated_recommendation_request(
            &self,
            _request: &CuratedRecommendationsRequest,
            _user_agent_header: &str,
            _base_host: Url,
        ) -> Result<CuratedRecommendationsResponse> {
            Err(Error::BadRequest {
                code: 400,
                message: "Invalid syntax".to_string(),
            })
        }
    }

    struct FakeCapturingClient {
        captured_url: std::sync::Arc<std::sync::Mutex<Option<Url>>>,
    }

    impl http::HttpClientTrait for FakeCapturingClient {
        fn make_curated_recommendation_request(
            &self,
            _request: &CuratedRecommendationsRequest,
            _user_agent_header: &str,
            url: Url,
        ) -> Result<CuratedRecommendationsResponse> {
            let mut lock = self.captured_url.lock().unwrap();
            *lock = Some(url);
            Err(Error::Unexpected {
                code: 999,
                message: "test error".into(),
            })
        }
    }

    #[test]
    fn test_get_curated_recommendations_success() {
        let fake_client = FakeHttpClientSuccess;
        let client_inner = CuratedRecommendationsClientInner::new_with_client(fake_client);

        let request = CuratedRecommendationsRequest {
            locale: CuratedRecommendationLocale::EnUs,
            region: Some("US".parse().unwrap()),
            count: Option::from(4),
            topics: Some(vec!["business".into()]),
            feeds: Some(vec!["sections".into()]),
            sections: Some(vec![SectionSettings {
                section_id: "d471863a-4ee9-4849-aff8-da087778b383".to_string(),
                is_followed: true,
                is_blocked: true,
            }]),
            experiment_name: Some("new-tab-extend-content-duration".parse().unwrap()),
            experiment_branch: None,
            enable_interest_picker: false,
        };

        let response_result = client_inner.get_curated_recommendations(
            &request,
            "Rust-HTTP-Client/0.1",
            &Url::parse("https://merino.services.mozilla.com").unwrap(),
        );

        assert!(response_result.is_ok(), "Expected a successful response");

        let response = response_result.unwrap();
        assert_eq!(response.recommended_at, 1740764371347);
        assert_eq!(response.data.len(), 4);

        let first_item = &response.data[0];
        let second_item = &response.data[1];
        let third_item = &response.data[2];
        let fourth_item = &response.data[3];

        assert_eq!(
            first_item.corpus_item_id,
            "18fbf4e1-3a8b-4b28-84a0-b6b4b785a44b"
        );
        assert_eq!(
            first_item.url,
            "https://getpocket.com/explore/item/how-online-influencers-got-addicted-to-swedish-candy?utm_source=firefox-newtab-en-us"
        );
        assert_eq!(
            first_item.title,
            "How Online Influencers Got Addicted to Swedish Candy"
        );

        assert_eq!(
            second_item.scheduled_corpus_item_id,
            "2de2453f-b38b-4713-8dcb-82660856efd7"
        );
        assert_eq!(second_item.icon_url, Option::from("https://merino-images.services.mozilla.com/favicons/ccd270c8c839b5560cc10386689067bbdbcedc437c0f6f5caa2db3a4c69eb01c_4792.svg".to_string()));

        assert_eq!(third_item.publisher, "NBC News".to_string());
        assert!(third_item.is_time_sensitive);

        assert!(fourth_item.is_time_sensitive);
        assert_eq!(fourth_item.topic, Option::from("government".to_string()));
    }

    #[test]
    fn test_get_curated_recommendations_validation_error() {
        let fake_client = FakeHttpClientValidationError;
        let client_inner = CuratedRecommendationsClientInner::new_with_client(fake_client);

        let request = CuratedRecommendationsRequest {
            locale: CuratedRecommendationLocale::Fr,
            region: None,
            count: None,
            topics: None,
            feeds: None,
            sections: None,
            experiment_name: None,
            experiment_branch: None,
            enable_interest_picker: false,
        };

        let response = client_inner.get_curated_recommendations(
            &request,
            "Rust-HTTP-Client/0.1",
            &Url::parse("https://merino.services.mozilla.com").unwrap(),
        );
        assert!(response.is_err());

        let err = response.unwrap_err();

        match err {
            Error::Validation { code, message } => {
                assert_eq!(code, 422);
                assert_eq!(message, "Invalid input");
            }
            _ => panic!("Expected a validation error"),
        }
    }

    #[test]
    fn test_get_curated_recommendations_server_error() {
        let fake_client = FakeHttpClientServerError;
        let client_inner = CuratedRecommendationsClientInner::new_with_client(fake_client);

        let request = CuratedRecommendationsRequest {
            locale: CuratedRecommendationLocale::Fr,
            region: None,
            count: None,
            topics: None,
            feeds: None,
            sections: None,
            experiment_name: None,
            experiment_branch: None,
            enable_interest_picker: false,
        };

        let response = client_inner.get_curated_recommendations(
            &request,
            "Rust-HTTP-Client/0.1",
            &Url::parse("https://merino.services.mozilla.com").unwrap(),
        );
        assert!(response.is_err());

        let err = response.unwrap_err();

        match err {
            Error::Server { code, message } => {
                assert_eq!(code, 500);
                assert_eq!(message, "The server encountered an unexpected error");
            }
            _ => panic!("Expected a server error"),
        }
    }

    #[test]
    fn test_get_curated_recommendations_bad_request_error() {
        let fake_client = FakeHttpClientBadRequestError;
        let client_inner = CuratedRecommendationsClientInner::new_with_client(fake_client);

        let request = CuratedRecommendationsRequest {
            locale: CuratedRecommendationLocale::Fr,
            region: None,
            count: None,
            topics: None,
            feeds: None,
            sections: None,
            experiment_name: None,
            experiment_branch: None,
            enable_interest_picker: false,
        };

        let response = client_inner.get_curated_recommendations(
            &request,
            "Rust-HTTP-Client/0.1",
            &Url::parse("https://merino.services.mozilla.com").unwrap(),
        );
        assert!(response.is_err());

        let err = response.unwrap_err();

        match err {
            Error::BadRequest { code, message } => {
                assert_eq!(code, 400);
                assert_eq!(message, "Invalid syntax");
            }
            _ => panic!("Expected a bad request error"),
        }
    }

    #[test]
    fn test_client_builder_with_default_base_host() {
        let config = CuratedRecommendationsConfig {
            base_host: None,
            user_agent_header: "test-agent/1.0".to_string(),
        };

        let client = CuratedRecommendationsClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_builder_uses_default_base_host_if_none_provided() {
        let captured_url = std::sync::Arc::new(std::sync::Mutex::new(None));
        let client_inner =
            CuratedRecommendationsClientInner::new_with_client(FakeCapturingClient {
                captured_url: captured_url.clone(),
            });

        let config = CuratedRecommendationsConfig {
            base_host: None,
            user_agent_header: "agent/1.0".into(),
        };

        let builder =
            CuratedRecommendationsClientBuilder::new().user_agent_header(config.user_agent_header);

        let client = builder.build().unwrap();

        let _ = client_inner.get_curated_recommendations(
            &CuratedRecommendationsRequest {
                locale: CuratedRecommendationLocale::EnUs,
                region: None,
                count: None,
                topics: None,
                feeds: None,
                sections: None,
                experiment_name: None,
                experiment_branch: None,
                enable_interest_picker: false,
            },
            &client.user_agent_header,
            &client.endpoint_url,
        );

        let captured = captured_url.lock().unwrap();
        assert_eq!(
            captured.as_ref().unwrap().as_str(),
            "https://merino.services.mozilla.com/api/v1/curated-recommendations"
        );
    }

    #[test]
    fn test_builder_uses_custom_base_host_if_provided() {
        let captured_url = std::sync::Arc::new(std::sync::Mutex::new(None));
        let client_inner =
            CuratedRecommendationsClientInner::new_with_client(FakeCapturingClient {
                captured_url: captured_url.clone(),
            });

        let base_host = "https://my.custom.host";
        let config = CuratedRecommendationsConfig {
            base_host: Some(base_host.to_string()),
            user_agent_header: "agent/1.0".into(),
        };

        let builder = CuratedRecommendationsClientBuilder::new()
            .user_agent_header(config.user_agent_header)
            .base_host(config.base_host.clone().unwrap());

        let client = builder.build().unwrap();

        let _ = client_inner.get_curated_recommendations(
            &CuratedRecommendationsRequest {
                locale: CuratedRecommendationLocale::EnUs,
                region: None,
                count: None,
                topics: None,
                feeds: None,
                sections: None,
                experiment_name: None,
                experiment_branch: None,
                enable_interest_picker: false,
            },
            &client.user_agent_header,
            &client.endpoint_url,
        );

        let captured = captured_url.lock().unwrap();
        assert_eq!(
            captured.as_ref().unwrap().as_str(),
            "https://my.custom.host/api/v1/curated-recommendations"
        );
    }
}
