/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::client::SettingsClient;
use crate::{Branch, BucketConfig, Experiment, FeatureConfig, RandomizationUnit, SCHEMA_VERSION};
use mockito::mock;
use rs_client::{Client, ClientConfig};

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
    let config = ClientConfig {
        server_url: Some(mockito::server_url()),
        collection_name: "messaging-experiments".to_string(),
        bucket_name: None,
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
