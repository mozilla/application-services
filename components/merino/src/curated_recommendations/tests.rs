/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

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
                    title: "TEST Cat Accidentally Summons Eldritch Horror While Playing With String".to_string(),
                    excerpt: "TikTok's obsession with Scandinavian sweets, which began in early 2024, has squeezed global supply chains and shows no signs of slowing down.".to_string(),
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
                    title: "TEST Breaking News: Even Moderately Maintained Roofs (Rooves?) Continue to Protect From Elements".to_string(),
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
                    title: "TEST 'We're Not Prepared': The World Braces for Man to Step Into the Same River Twice".to_string(),
                    excerpt: "Trump has said he wants school policy to be left to the states, but state officials and lawmakers aren't clear on what that would look like.  ".to_string(),
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
                    title: "TEST Dogs and cats get along after centuries of fighting".to_string(),
                    excerpt: "The US president calls his Ukrainian counterpart \"disrespectful\" and tells him to be \"thankful\" during heated exchanges in the Oval Office.".to_string(),
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
        "TEST Cat Accidentally Summons Eldritch Horror While Playing With String"
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
    let client_inner = CuratedRecommendationsClientInner::new_with_client(FakeCapturingClient {
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
    let client_inner = CuratedRecommendationsClientInner::new_with_client(FakeCapturingClient {
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

// --- Request serialization tests (#3) ---

#[test]
fn test_request_serialization_camel_case_fields() {
    let request = CuratedRecommendationsRequest {
        locale: CuratedRecommendationLocale::EnUs,
        region: Some("US".to_string()),
        count: Some(10),
        topics: Some(vec!["business".into()]),
        feeds: None,
        sections: None,
        experiment_name: Some("test-experiment".to_string()),
        experiment_branch: Some("control".to_string()),
        enable_interest_picker: true,
    };

    let json = serde_json::to_value(&request).unwrap();

    // Verify camelCase renames are applied
    assert_eq!(json["experimentName"], "test-experiment");
    assert_eq!(json["experimentBranch"], "control");
    assert_eq!(json["enableInterestPicker"], true);

    // Verify these snake_case keys do NOT appear
    assert!(json.get("experiment_name").is_none());
    assert!(json.get("experiment_branch").is_none());
    assert!(json.get("enable_interest_picker").is_none());
}

#[test]
fn test_request_serialization_locale_value() {
    let request = CuratedRecommendationsRequest {
        locale: CuratedRecommendationLocale::FrFr,
        region: None,
        count: None,
        topics: None,
        feeds: None,
        sections: None,
        experiment_name: None,
        experiment_branch: None,
        enable_interest_picker: false,
    };

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["locale"], "fr-FR");
}

#[test]
fn test_request_serialization_sections() {
    let request = CuratedRecommendationsRequest {
        locale: CuratedRecommendationLocale::EnUs,
        region: None,
        count: None,
        topics: None,
        feeds: None,
        sections: Some(vec![SectionSettings {
            section_id: "abc-123".to_string(),
            is_followed: true,
            is_blocked: false,
        }]),
        experiment_name: None,
        experiment_branch: None,
        enable_interest_picker: false,
    };

    let json = serde_json::to_value(&request).unwrap();
    let section = &json["sections"][0];

    // Verify camelCase renames on SectionSettings
    assert_eq!(section["sectionId"], "abc-123");
    assert_eq!(section["isFollowed"], true);
    assert_eq!(section["isBlocked"], false);
    assert!(section.get("section_id").is_none());
}

// --- Builder edge case tests (#4) ---

#[test]
fn test_builder_fails_without_user_agent_header() {
    let result = CuratedRecommendationsClientBuilder::new().build();

    match result {
        Err(Error::Unexpected { message, .. }) => {
            assert!(message.contains("user_agent_header"));
        }
        Err(other) => panic!("Expected Unexpected error, got: {:?}", other),
        Ok(_) => panic!("Expected error for missing user_agent_header"),
    }
}

#[test]
fn test_builder_fails_with_invalid_base_host() {
    let result = CuratedRecommendationsClientBuilder::new()
        .user_agent_header("agent/1.0")
        .base_host("not a valid url")
        .build();

    match result {
        Err(Error::UrlParse(_)) => {}
        Err(other) => panic!("Expected UrlParse error, got: {:?}", other),
        Ok(_) => panic!("Expected error for invalid base_host"),
    }
}

#[test]
fn test_client_new_with_empty_user_agent() {
    // Empty string is accepted — the builder only requires user_agent_header to be present.
    let config = CuratedRecommendationsConfig {
        base_host: None,
        user_agent_header: "".to_string(),
    };

    let result = CuratedRecommendationsClient::new(config);
    assert!(result.is_ok());
}
