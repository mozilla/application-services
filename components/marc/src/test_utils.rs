/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use crate::{
    error::Result,
    mars::{DefaultMARSClient, MARSClient},
    models::{AdCallbacks, AdRequest, AdResponse, MozAd},
};

#[allow(dead_code)]
pub const TEST_CONTEXT_ID: &str = "test-context-id";

#[allow(dead_code)]
pub fn get_example_happy_ad_response() -> AdResponse {
    AdResponse {
        data: HashMap::from([
            (
                "example_placement_1".to_string(),
                vec![MozAd {
                    url: Some("https://ads.fakeexample.org/example_ad_1".to_string()),
                    image_url: Some("https://ads.fakeexample.org/example_image_1".to_string()),
                    format: Some("billboard".to_string()),
                    block_key: None,
                    alt_text: Some("An ad for a puppy".to_string()),
                    callbacks: Some(AdCallbacks {
                        click: Some("https://ads.fakeexample.org/click/example_ad_1".to_string()),
                        impression: Some(
                            "https://ads.fakeexample.org/impression/example_ad_1".to_string(),
                        ),
                        report: Some("https://ads.fakeexample.org/report/example_ad_1".to_string()),
                    }),
                }],
            ),
            (
                "example_placement_2".to_string(),
                vec![MozAd {
                    url: Some("https://ads.fakeexample.org/example_ad_2".to_string()),
                    image_url: Some("https://ads.fakeexample.org/example_image_2".to_string()),
                    format: Some("skyscraper".to_string()),
                    block_key: None,
                    alt_text: Some("An ad for a pet duck".to_string()),
                    callbacks: Some(AdCallbacks {
                        click: Some("https://ads.fakeexample.org/click/example_ad_2".to_string()),
                        impression: Some(
                            "https://ads.fakeexample.org/impression/example_ad_2".to_string(),
                        ),
                        report: Some("https://ads.fakeexample.org/report/example_ad_2".to_string()),
                    }),
                }],
            ),
        ]),
    }
}

#[allow(dead_code)]
pub fn get_example_missing_callback_ad_response() -> AdResponse {
    AdResponse {
        data: HashMap::from([
            (
                "example_placement_1".to_string(),
                vec![MozAd {
                    url: Some("https://ads.fakeexample.org/example_ad_1".to_string()),
                    image_url: Some("https://ads.fakeexample.org/example_image_1".to_string()),
                    format: Some("billboard".to_string()),
                    block_key: None,
                    alt_text: Some("An ad for a puppy".to_string()),
                    callbacks: Some(AdCallbacks {
                        click: Some("https://ads.fakeexample.org/click/example_ad_1".to_string()),
                        impression: None, // Missing impression callback URL
                        report: Some("https://ads.fakeexample.org/report/example_ad_1".to_string()),
                    }),
                }],
            ),
            (
                "example_placement_2".to_string(),
                vec![MozAd {
                    url: Some("https://ads.fakeexample.org/example_ad_2".to_string()),
                    image_url: Some("https://ads.fakeexample.org/example_image_2".to_string()),
                    format: Some("skyscraper".to_string()),
                    block_key: None,
                    alt_text: Some("An ad for a pet duck".to_string()),
                    callbacks: Some(AdCallbacks {
                        click: Some("https://ads.fakeexample.org/click/example_ad_2".to_string()),
                        impression: Some(
                            "https://ads.fakeexample.org/impression/example_ad_2".to_string(),
                        ),
                        report: Some("https://ads.fakeexample.org/report/example_ad_2".to_string()),
                    }),
                }],
            ),
        ]),
    }
}

#[allow(dead_code)]
pub fn create_test_client(test_endpoint: String) -> TestClientWithCustomURL {
    TestClientWithCustomURL {
        inner: DefaultMARSClient::new(TEST_CONTEXT_ID.to_string()),
        endpoint: test_endpoint,
    }
}

pub struct TestClientWithCustomURL {
    inner: DefaultMARSClient,
    endpoint: String,
}

impl MARSClient for TestClientWithCustomURL {
    fn get_mars_endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn fetch_ads(&self, req: &AdRequest) -> Result<AdResponse> {
        self.inner.fetch_ads(req)
    }

    fn record_impression(&self, url: Option<&String>) -> Result<()> {
        self.inner.record_impression(url)
    }

    fn record_click(&self, url: Option<&String>) -> Result<()> {
        self.inner.record_click(url)
    }

    fn record_report_ad(&self, url: Option<&String>) -> Result<()> {
        self.inner.record_report_ad(url)
    }

    fn get_context_id(&self) -> &str {
        self.inner.get_context_id()
    }

    fn cycle_context_id(&mut self) -> String {
        self.inner.cycle_context_id()
    }
}
