/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This is a simple HTTP client that uses viaduct to retrieve experiment data from the server.
//! Currently configured to use Kinto and the old schema, although that would change once we start
//! working on the real Nimbus schema.
//!
//! In the future we might replace this with a more fully-feature Remote Settings client, such as:
//!
//!   https://github.com/mozilla-services/remote-settings-client
//!   Issue: https://github.com/mozilla/application-services/issues/3475
//!
//! But the simple subset implemented here meets our needs for now.

use super::Experiment;
use crate::config::RemoteSettingsConfig;
use crate::error::{Error, Result};
use url::Url;
use viaduct::{status_codes, Request, Response};

// Making this a trait so that we can mock those later.
pub(crate) trait SettingsClient {
    fn get_experiments_metadata(&self) -> Result<String>;
    fn get_experiments(&self) -> Result<Vec<Experiment>>;
}

pub struct Client {
    base_url: Url,
    collection_name: String,
    bucket_name: String,
}

impl Client {
    #[allow(unused)]
    pub fn new(config: RemoteSettingsConfig) -> Result<Self> {
        let base_url = Url::parse(&config.server_url)?;
        Ok(Self {
            base_url,
            collection_name: config.collection_name,
            bucket_name: config.bucket_name,
        })
    }

    fn make_request(&self, request: Request) -> Result<Response> {
        let resp = request.send()?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            Err(Error::ResponseError(resp.text().to_string()))
        }
    }
}

impl SettingsClient for Client {
    fn get_experiments_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn get_experiments(&self) -> Result<Vec<Experiment>> {
        let path = format!(
            "buckets/{}/collections/{}/records",
            &self.bucket_name, &self.collection_name
        );
        let url = self.base_url.join(&path)?;
        let req = Request::get(url);
        // We first encode the response into a `serde_json::Value`
        // to allow us to deserialize each experiment individually,
        // omitting any malformed experiments
        let resp = self.make_request(req)?.json::<serde_json::Value>()?;
        let data = resp.get("data").ok_or(Error::InvalidExperimentResponse)?;
        let mut res = Vec::new();
        for exp in data.as_array().ok_or(Error::InvalidExperimentResponse)? {
            match serde_json::from_value::<Experiment>(exp.clone()) {
                Ok(exp) => res.push(exp),
                Err(e) => log::warn!(
                    "Malformed experiment found! Experiment {},  Error: {}",
                    exp.get("id").unwrap_or(&serde_json::json!("ID_NOT_FOUND")),
                    e
                ),
            }
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Branch, BucketConfig, RandomizationUnit};
    use mockito::mock;

    #[test]
    fn test_get_experiments_from_schema() {
        viaduct_reqwest::use_reqwest_backend();
        let body = r#"
        { "data": [
            {
                "slug": "mobile-a-a-example",
                "application": "reference-browser",
                "userFacingName": "Mobile A/A Example",
                "userFacingDescription": "An A/A Test to validate the Rust SDK",
                "isEnrollmentPaused": false,
                "bucketConfig": {
                    "randomizationUnit": "nimbus_id",
                    "namespace": "mobile-a-a-example",
                    "start": 0,
                    "count": 5000,
                    "total": 10000
                },
                "startDate": null,
                "endDate": null,
                "proposedEnrollment": 7,
                "referenceBranch": "control",
                "probeSets": [],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment-variation-b",
                    "ratio": 1
                    }
                ]
            },
            {
                "hello": "bye"
            }
        ]}
          "#;
        let m = mock(
            "GET",
            "/buckets/main/collections/messaging-experiments/records",
        )
        .with_body(body)
        .with_status(200)
        .with_header("content-type", "application/json")
        .create();
        let config = RemoteSettingsConfig {
            server_url: mockito::server_url(),
            collection_name: "messaging-experiments".to_string(),
            bucket_name: "main".to_string(),
        };
        let http_client = Client::new(config).unwrap();
        let resp = http_client.get_experiments().unwrap();
        m.expect(1).assert();
        assert_eq!(resp.len(), 1);
        let exp = &resp[0];
        assert_eq!(
            exp.clone(),
            Experiment {
                slug: "mobile-a-a-example".to_string(),
                application: "reference-browser".to_string(),
                user_facing_name: "Mobile A/A Example".to_string(),
                user_facing_description: "An A/A Test to validate the Rust SDK".to_string(),
                is_enrollment_paused: false,
                bucket_config: BucketConfig {
                    randomization_unit: RandomizationUnit::NimbusId,
                    namespace: "mobile-a-a-example".to_string(),
                    start: 0,
                    count: 5000,
                    total: 10000
                },
                start_date: None,
                end_date: None,
                proposed_duration: None,
                proposed_enrollment: 7,
                reference_branch: Some("control".to_string()),
                probe_sets: vec![],
                branches: vec![
                    Branch {
                        slug: "control".to_string(),
                        ratio: 1,
                        feature: None
                    },
                    Branch {
                        slug: "treatment-variation-b".to_string(),
                        ratio: 1,
                        feature: None
                    },
                ],
                targeting: None,
            }
        )
    }
}
