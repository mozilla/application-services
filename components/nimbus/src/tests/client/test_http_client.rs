/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::client::{http_client::*, SettingsClient};
use crate::{
    error::NimbusError, Branch, BucketConfig, Experiment, FeatureConfig, RandomizationUnit,
    RemoteSettingsConfig, SCHEMA_VERSION,
};
use mockito::mock;
use std::cell::Cell;
use std::time::{Duration, Instant};

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
            proposed_enrollment: 7,
            reference_branch: Some("control".to_string()),
            feature_ids: vec!["first_switch".to_string()],
            branches: vec![
                Branch {
                    slug: "control".to_string(),
                    ratio: 1,
                    feature: Some(FeatureConfig {
                        feature_id: "first_switch".to_string(),
                        value: Default::default(),
                    }),
                    features: None,
                },
                Branch {
                    slug: "treatment-variation-b".to_string(),
                    ratio: 1,
                    feature: Some(FeatureConfig {
                        feature_id: "first_switch".to_string(),
                        value: Default::default(),
                    }),
                    features: None,
                },
            ],
            ..Default::default()
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
