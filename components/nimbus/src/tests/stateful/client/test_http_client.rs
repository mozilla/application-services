/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::schema::parse_experiments;
use crate::{Branch, BucketConfig, Experiment, FeatureConfig, NimbusError, RandomizationUnit};

#[test]
fn test_fetch_experiments_from_schema() {
    // There are three experiments defined here, one has a "newer" schema version
    // in order to test filtering of unsupported schema versions, one is malformed, and one
    // should parse correctly.
    let result = parse_experiments(&response_body()).unwrap();

    assert_eq!(result.len(), 2);
    let exp = &result[0];
    assert_eq!(
        exp.clone(),
        Experiment {
            schema_version: "1.0.0".to_string(),
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
fn test_malformed_payload() {
    let payload = r#"
        { "datar" : [} ]]
    "#;

    let result = parse_experiments(payload).unwrap_err();
    assert!(matches!(result, NimbusError::JSONError(_, _)));
}

// This response body includes a matching schema version, a non-matching schema version,
// and a malformed experiment.
fn response_body() -> String {
    r#"{ "data": [
        {
            "schemaVersion": "1.0.0",
            "slug": "mobile-a-a-example",
            "appName": "reference-browser",
            "channel": "nightly",
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
            "featureIds": ["first_switch"],
            "branches": [
                {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "first_switch",
                    "enabled": false
                    }
                },
                {
                "slug": "treatment-variation-b",
                "ratio": 1,
                "feature": {
                    "featureId": "first_switch",
                    "enabled": true
                    }
                }
            ]
        },
        {
            "schemaVersion": "2.0.0",
            "slug": "mobile-a-a-example",
            "appName": "reference-browser",
            "channel": "nightly",
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
            "featureIds": ["some_switch"],
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
            "slug": "schema-version-missing",
            "appName": "reference-browser",
            "userFacingName": "Schema Version Missing",
            "userFacingDescription": "This should be completely ignored",
            "isEnrollmentPaused": false
        }
    ]}"#
    .to_string()
}
