/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This is a simple Http client that uses viaduct to retrieve experiment data from the server
//! Currently configured to use Kinto and the old schema, although that would change once we start
//! Working on the real Nimbus schema.

use super::Experiment;
use anyhow::Result;
use url::Url;
use viaduct::{status_codes, Request, Response};

// Making this a trait so that we can mock those later.
pub(crate) trait SettingsClient {
    fn get_experiements_metadata(&self) -> Result<String>;
    fn get_experiments(&self) -> Result<Vec<Experiment>>;
}

pub struct Client {
    base_url: Url,
    collection_name: String,
    bucket_name: String,
}

impl Client {
    #[allow(unused)]
    pub fn new(base_url: Url, collection_name: String, bucket_name: String) -> Self {
        Self {
            base_url,
            collection_name,
            bucket_name,
        }
    }

    fn make_request(&self, request: Request) -> Result<Response> {
        let resp = request.send()?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            anyhow::bail!("Error in request: {}", resp.text())
        }
    }
}

impl SettingsClient for Client {
    fn get_experiements_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn get_experiments(&self) -> Result<Vec<Experiment>> {
        let path = format!(
            "buckets/{}/collections/{}/records",
            &self.bucket_name, &self.collection_name
        );
        let url = self.base_url.join(&path)?;
        let req = Request::get(url);
        let resp = self.make_request(req)?.json::<Vec<Experiment>>()?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Branch, BranchValue, BucketConfig, ExperimentArguments, Group, RandomizationUnit};
    use mockito::mock;

    #[test]
    fn test_get_experiments_from_schema() {
        viaduct_reqwest::use_reqwest_backend();
        let body = r#"
        [
            {
                "id": "ABOUTWELCOME-PULL-FACTOR-REINFORCEMENT-76-RELEASE",
                "enabled": true,
                "filter_expression": "(env.version >= '76.' && env.version < '77.' && env.channel == 'release' && !(env.telemetry.main.environment.profile.creationDate < ('2020-05-13'|date / 1000 / 60 / 60 / 24))) || (locale == 'en-US' && [userId, \"aboutwelcome-pull-factor-reinforcement-76-release\"]|bucketSample(0, 2000, 10000) && (!('trailhead.firstrun.didSeeAboutWelcome'|preferenceValue) || 'bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77' in activeExperiments))",
                "arguments": {
                    "slug": "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77",
                    "userFacingName": "About:Welcome Pull Factor Reinforcement",
                    "userFacingDescription": "4 branch experiment different variants of about:welcome with a goal of testing new experiment framework and get insights on whether reinforcing pull-factors improves retention. Test deployment of multiple branches using new experiment framework",
                    "isEnrollmentPaused": true,
                    "active": true,
                    "bucketConfig": {
                        "randomizationUnit": "normandy_id",
                        "namespace": "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77",
                        "start": 0,
                        "count": 2000,
                        "total": 10000
                    },
                    "startDate": "2020-06-17T23:20:47.230Z",
                    "endDate": null,
                    "proposedDuration": 28,
                    "proposedEnrollment": 7,
                    "referenceBranch": "control",
                    "features": [],
                    "branches": [
                        { "slug": "control", "ratio": 1, "value": {}, "group": ["cfr"] },
                        { "slug": "treatment-variation-b", "ratio": 1, "value": {} }
                    ]
                }
            }
        ]
          "#;
        let m = mock(
            "GET",
            "/buckets/main/collections/messaging-collection/records",
        )
        .with_body(body)
        .with_status(200)
        .with_header("content-type", "application/json")
        .create();
        let http_client = Client::new(
            Url::parse(&mockito::server_url()).unwrap(),
            "messaging-collection".to_string(),
            "main".to_string(),
        );
        let resp = http_client.get_experiments().unwrap();
        m.expect(1).assert();
        assert_eq!(resp.len(), 1);
        let exp = &resp[0];
        assert_eq!(exp.clone(), Experiment {
            id: "ABOUTWELCOME-PULL-FACTOR-REINFORCEMENT-76-RELEASE".to_string(),
            enabled: true,
            filter_expression: "(env.version >= '76.' && env.version < '77.' && env.channel == 'release' && !(env.telemetry.main.environment.profile.creationDate < ('2020-05-13'|date / 1000 / 60 / 60 / 24))) || (locale == 'en-US' && [userId, \"aboutwelcome-pull-factor-reinforcement-76-release\"]|bucketSample(0, 2000, 10000) && (!('trailhead.firstrun.didSeeAboutWelcome'|preferenceValue) || 'bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77' in activeExperiments))".to_string(),
            arguments: ExperimentArguments {
                    slug: "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77".to_string(),
                    user_facing_name: "About:Welcome Pull Factor Reinforcement".to_string(),
                    user_facing_description: "4 branch experiment different variants of about:welcome with a goal of testing new experiment framework and get insights on whether reinforcing pull-factors improves retention. Test deployment of multiple branches using new experiment framework".to_string(),
                    is_enrollment_paused: true,
                    active: true,
                    bucket_config: BucketConfig {
                        randomization_unit: RandomizationUnit::NormandyId,
                        namespace: "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77".to_string(),
                        start: 0,
                        count: 2000,
                        total: 10000
                    },
                    start_date: serde_json::from_str("\"2020-06-17T23:20:47.230Z\"").unwrap(),
                    end_date: None,
                    proposed_duration: 28,
                    proposed_enrollment: 7,
                    reference_branch: Some("control".to_string()),
                    features: vec![],
                    branches: vec![
                        Branch { slug: "control".to_string(), ratio: 1, value: BranchValue {}, group: Some(vec![Group::Cfr]) },
                        Branch { slug: "treatment-variation-b".to_string(), ratio: 1, value: BranchValue {}, group: None }
                    ]
                },
            targeting: None,
        })
    }
}
