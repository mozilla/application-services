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

use std::time::{Duration, Instant};

use crate::config::RemoteSettingsConfig;
use crate::error::{NimbusError, Result};
use crate::{Experiment, SettingsClient, SCHEMA_VERSION};
use std::cell::Cell;
use url::Url;
use viaduct::{status_codes, Request, Response};

const HEADER_BACKOFF: &str = "Backoff";
const HEADER_RETRY_AFTER: &str = "Retry-After";

pub struct Client {
    base_url: Url,
    collection_name: String,
    remote_state: Cell<RemoteState>,
}

#[derive(Clone, Copy, Debug)]
enum RemoteState {
    Ok,
    Backoff {
        observed_at: Instant,
        duration: Duration,
    },
}

impl Client {
    #[allow(unused)]
    pub fn new(config: RemoteSettingsConfig) -> Result<Self> {
        let base_url = Url::parse(&config.server_url)?;
        Ok(Self {
            base_url,
            collection_name: config.collection_name,
            remote_state: Cell::new(RemoteState::Ok),
        })
    }

    fn make_request(&self, request: Request) -> Result<Response> {
        self.ensure_no_backoff()?;
        let resp = request.send()?;
        self.handle_backoff_hint(&resp)?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            Err(NimbusError::ResponseError(resp.text().to_string()))
        }
    }

    fn ensure_no_backoff(&self) -> Result<()> {
        if let RemoteState::Backoff {
            observed_at,
            duration,
        } = self.remote_state.get()
        {
            let elapsed_time = observed_at.elapsed();
            if elapsed_time >= duration {
                self.remote_state.replace(RemoteState::Ok);
            } else {
                let remaining = duration - elapsed_time;
                return Err(NimbusError::BackoffError(remaining.as_secs()));
            }
        }
        Ok(())
    }

    fn handle_backoff_hint(&self, response: &Response) -> Result<()> {
        let extract_backoff_header = |header| -> Result<u64> {
            Ok(response
                .headers
                .get_as::<u64, _>(header)
                .transpose()
                .unwrap_or_default() // Ignore number parsing errors.
                .unwrap_or(0))
        };
        // In practice these two headers are mutually exclusive.
        let backoff = extract_backoff_header(HEADER_BACKOFF)?;
        let retry_after = extract_backoff_header(HEADER_RETRY_AFTER)?;
        let max_backoff = backoff.max(retry_after);

        if max_backoff > 0 {
            self.remote_state.replace(RemoteState::Backoff {
                observed_at: Instant::now(),
                duration: Duration::from_secs(max_backoff),
            });
        }
        Ok(())
    }
}

impl SettingsClient for Client {
    fn get_experiments_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn fetch_experiments(&self) -> Result<Vec<Experiment>> {
        let path = format!(
            "v1/buckets/main/collections/{}/records",
            &self.collection_name
        );
        let url = self.base_url.join(&path)?;
        let req = Request::get(url);
        let resp = self.make_request(req)?;
        parse_experiments(&resp.text())
    }
}

pub fn parse_experiments(payload: &str) -> Result<Vec<Experiment>> {
    // We first encode the response into a `serde_json::Value`
    // to allow us to deserialize each experiment individually,
    // omitting any malformed experiments
    let value: serde_json::Value = serde_json::from_str(payload)?;
    let data = value
        .get("data")
        .ok_or(NimbusError::InvalidExperimentFormat)?;
    let mut res = Vec::new();
    for exp in data
        .as_array()
        .ok_or(NimbusError::InvalidExperimentFormat)?
    {
        // Validate the schema major version matches the supported version
        let exp_schema_version = match exp.get("schemaVersion") {
            Some(ver) => {
                serde_json::from_value::<String>(ver.to_owned()).unwrap_or_else(|_| "".to_string())
            }
            None => {
                log::trace!("Missing schemaVersion: {:#?}", exp);
                continue;
            }
        };
        let schema_maj_version = exp_schema_version.split('.').next().unwrap_or("");
        // While "0" is a valid schema version, we have already passed that so reserving zero as
        // a special value here in order to avoid a panic, and just ignore the experiment.
        let schema_version: u32 = schema_maj_version.parse().unwrap_or(0);
        if schema_version != SCHEMA_VERSION {
            log::info!(
                    "Schema version mismatch: Expected version {}, discarding experiment with version {}",
                    SCHEMA_VERSION, schema_version
                );
            // Schema version mismatch
            continue;
        }

        match serde_json::from_value::<Experiment>(exp.clone()) {
            Ok(exp) => res.push(exp),
            Err(e) => {
                log::trace!("Malformed experiment data: {:#?}", exp);
                log::warn!(
                    "Malformed experiment found! Experiment {},  Error: {}",
                    exp.get("id").unwrap_or(&serde_json::json!("ID_NOT_FOUND")),
                    e
                );
            }
        }
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Branch, BucketConfig, FeatureConfig, RandomizationUnit};
    use mockito::mock;

    fn response_body() -> String {
        format!(
            r#"
        {{ "data": [
            {{
                "schemaVersion": "{current_version}.0.0",
                "slug": "mobile-a-a-example",
                "appName": "reference-browser",
                "channel": "nightly",
                "userFacingName": "Mobile A/A Example",
                "userFacingDescription": "An A/A Test to validate the Rust SDK",
                "isEnrollmentPaused": false,
                "bucketConfig": {{
                    "randomizationUnit": "nimbus_id",
                    "namespace": "mobile-a-a-example",
                    "start": 0,
                    "count": 5000,
                    "total": 10000
                }},
                "startDate": null,
                "endDate": null,
                "proposedEnrollment": 7,
                "referenceBranch": "control",
                "probeSets": [],
                "featureIds": ["first_switch"],
                "branches": [
                    {{
                    "slug": "control",
                    "ratio": 1,
                    "feature": {{
                        "featureId": "first_switch",
                        "enabled": false
                        }}
                    }},
                    {{
                    "slug": "treatment-variation-b",
                    "ratio": 1,
                    "feature": {{
                        "featureId": "first_switch",
                        "enabled": true
                        }}
                    }}
                ]
            }},
            {{
                "schemaVersion": "{newer_version}.0.0",
                "slug": "mobile-a-a-example",
                "appName": "reference-browser",
                "channel": "nightly",
                "userFacingName": "Mobile A/A Example",
                "userFacingDescription": "An A/A Test to validate the Rust SDK",
                "isEnrollmentPaused": false,
                "bucketConfig": {{
                    "randomizationUnit": "nimbus_id",
                    "namespace": "mobile-a-a-example",
                    "start": 0,
                    "count": 5000,
                    "total": 10000
                }},
                "startDate": null,
                "endDate": null,
                "proposedEnrollment": 7,
                "referenceBranch": "control",
                "probeSets": [],
                "featureIds": ["some_switch"],
                "branches": [
                    {{
                    "slug": "control",
                    "ratio": 1
                    }},
                    {{
                    "slug": "treatment-variation-b",
                    "ratio": 1
                    }}
                ]
            }},
            {{
                "slug": "schema-version-missing",
                "appName": "reference-browser",
                "userFacingName": "Schema Version Missing",
                "userFacingDescription": "This should be completely ignored",
                "isEnrollmentPaused": false
            }}
        ]}}"#,
            current_version = SCHEMA_VERSION,
            newer_version = SCHEMA_VERSION + 1
        )
    }

    #[test]
    fn test_fetch_experiments_from_schema() {
        viaduct_reqwest::use_reqwest_backend();
        // There are two experiments defined here, one has a "newer" schema version
        // in order to test filtering of unsupported schema versions.
        let m = mock(
            "GET",
            "/v1/buckets/main/collections/messaging-experiments/records",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .create();
        let config = RemoteSettingsConfig {
            server_url: mockito::server_url(),
            collection_name: "messaging-experiments".to_string(),
        };
        let http_client = Client::new(config).unwrap();
        let resp = http_client.fetch_experiments().unwrap();

        m.expect(1).assert();
        assert_eq!(resp.len(), 1);
        let exp = &resp[0];
        assert_eq!(
            exp.clone(),
            Experiment {
                schema_version: format!("{}.0.0", SCHEMA_VERSION),
                slug: "mobile-a-a-example".to_string(),
                app_id: None,
                app_name: Some("reference-browser".to_string()),
                channel: Some("nightly".to_string()),
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
                feature_ids: vec!["first_switch".to_string()],
                branches: vec![
                    Branch {
                        slug: "control".to_string(),
                        ratio: 1,
                        feature: Some(FeatureConfig {
                            feature_id: "first_switch".to_string(),
                            enabled: false,
                            value: Default::default(),
                        }),
                        features: None,
                    },
                    Branch {
                        slug: "treatment-variation-b".to_string(),
                        ratio: 1,
                        feature: Some(FeatureConfig {
                            feature_id: "first_switch".to_string(),
                            enabled: true,
                            value: Default::default(),
                        }),
                        features: None,
                    },
                ],
                targeting: None
            }
        )
    }

    #[test]
    fn test_backoff() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock(
            "GET",
            "/v1/buckets/main/collections/messaging-experiments/records",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("Backoff", "60")
        .create();
        let config = RemoteSettingsConfig {
            server_url: mockito::server_url(),
            collection_name: "messaging-experiments".to_string(),
        };
        let http_client = Client::new(config).unwrap();
        assert!(http_client.fetch_experiments().is_ok());
        let second_request = http_client.fetch_experiments();
        assert!(matches!(second_request, Err(NimbusError::BackoffError(_))));
        m.expect(1).assert();
    }

    #[test]
    fn test_500_retry_after() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock(
            "GET",
            "/v1/buckets/main/collections/messaging-experiments/records",
        )
        .with_body("Boom!")
        .with_status(500)
        .with_header("Retry-After", "60")
        .create();
        let config = RemoteSettingsConfig {
            server_url: mockito::server_url(),
            collection_name: "messaging-experiments".to_string(),
        };
        let http_client = Client::new(config).unwrap();
        assert!(http_client.fetch_experiments().is_err());
        let second_request = http_client.fetch_experiments();
        assert!(matches!(second_request, Err(NimbusError::BackoffError(_))));
        m.expect(1).assert();
    }

    #[test]
    fn test_backoff_recovery() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock(
            "GET",
            "/v1/buckets/main/collections/messaging-experiments/records",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .create();
        let config = RemoteSettingsConfig {
            server_url: mockito::server_url(),
            collection_name: "messaging-experiments".to_string(),
        };
        let mut http_client = Client::new(config).unwrap();
        // First, sanity check that manipulating the remote state does something.
        http_client.remote_state.replace(RemoteState::Backoff {
            observed_at: Instant::now(),
            duration: Duration::from_secs(30),
        });
        assert!(matches!(
            http_client.fetch_experiments(),
            Err(NimbusError::BackoffError(_))
        ));
        // Then do the actual test.
        http_client.remote_state = Cell::new(RemoteState::Backoff {
            observed_at: Instant::now() - Duration::from_secs(31),
            duration: Duration::from_secs(30),
        });
        assert!(http_client.fetch_experiments().is_ok());
        m.expect(1).assert();
    }
}
