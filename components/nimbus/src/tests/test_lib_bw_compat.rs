/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{Branch, Experiment, FeatureConfig};
/// A suite of tests for b/w compat of data storage schema.
///
/// We use the `Serialize/`Deserialize` impls on various structs in order to persist them
/// into rkv, and it's important that we be able to read previously-persisted data even
/// if the struct definitions change over time.
///
/// This is a suite of tests specifically to check for backward compatibility with data
/// that may have been written to disk by previous versions of the library.
///
/// ⚠️ Warning : Do not change the JSON data used by these tests. ⚠️
/// ⚠️ The whole point of the tests is to check things work with that data. ⚠️
///
use serde_json::json;

#[test]
fn test_without_probe_sets_and_enabled() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    // this is an experiment following the schema after the removal
    // of the `enabled` and `probe_sets` fields which were removed
    // together in the same proposal
    serde_json::from_value::<Experiment>(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "appName": "fenix",
        "appId": "bobo",
        "channel": "nightly",
        "endDate": null,
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "features": [{
                    "featureId": "feature1",
                    "value": {
                        "key": "value1"
                    }
                },
                {
                    "featureId": "feature2",
                    "value": {
                        "key": "value2"
                    }
                }]
            },
            {
                "slug": "treatment",
                "ratio":1,
                "features": [{
                    "featureId": "feature3",
                    "value": {
                        "key": "value3"
                    }
                },
                {
                    "featureId": "feature4",
                    "value": {
                        "key": "value4"
                    }
                }]
            }
        ],
        "startDate":null,
        "application":"fenix",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
}

#[test]
fn test_multifeature_branch_schema() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    // this is an experiment following the schema after the addition
    // of multiple features per branch
    let exp: Experiment = serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "appName": "fenix",
        "appId": "bobo",
        "channel": "nightly",
        "endDate": null,
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "features": [{
                    "featureId": "feature1",
                    "enabled": true,
                    "value": {
                        "key": "value1"
                    }
                },
                {
                    "featureId": "feature2",
                    "enabled": false,
                    "value": {
                        "key": "value2"
                    }
                }]
            },
            {
                "slug": "treatment",
                "ratio":1,
                "features": [{
                    "featureId": "feature3",
                    "enabled": true,
                    "value": {
                        "key": "value3"
                    }
                },
                {
                    "featureId": "feature4",
                    "enabled": false,
                    "value": {
                        "key": "value4"
                    }
                }]
            }
        ],
        "probeSets":[],
        "startDate":null,
        "application":"fenix",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
    assert_eq!(
        exp.branches[0].get_feature_configs(),
        vec![
            FeatureConfig {
                feature_id: "feature1".to_string(),
                value: vec![("key".to_string(), json!("value1"))]
                    .into_iter()
                    .collect()
            },
            FeatureConfig {
                feature_id: "feature2".to_string(),
                value: vec![("key".to_string(), json!("value2"))]
                    .into_iter()
                    .collect()
            }
        ]
    );
    assert_eq!(
        exp.branches[1].get_feature_configs(),
        vec![
            FeatureConfig {
                feature_id: "feature3".to_string(),
                value: vec![("key".to_string(), json!("value3"))]
                    .into_iter()
                    .collect()
            },
            FeatureConfig {
                feature_id: "feature4".to_string(),
                value: vec![("key".to_string(), json!("value4"))]
                    .into_iter()
                    .collect()
            }
        ]
    );
    assert!(exp.branches[0].feature.is_none());
    assert!(exp.branches[1].feature.is_none());
}

#[test]
fn test_only_one_feature_branch_schema() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    // this is an experiment following the schema before the addition
    // of multiple features per branch
    let exp: Experiment = serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "appName": "fenix",
        "appId": "bobo",
        "channel": "nightly",
        "endDate": null,
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "feature1",
                    "enabled": true,
                    "value": {
                        "key": "value"
                    }
                }
            },
            {
                "slug": "treatment",
                "ratio":1,
                "feature": {
                    "featureId": "feature2",
                    "enabled": true,
                    "value": {
                        "key": "value2"
                    }
                }
            }
        ],
        "probeSets":[],
        "startDate":null,
        "application":"fenix",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
    assert_eq!(
        exp.branches[0].get_feature_configs(),
        vec![FeatureConfig {
            feature_id: "feature1".to_string(),
            value: vec![("key".to_string(), json!("value"))]
                .into_iter()
                .collect()
        }]
    );
    assert_eq!(
        exp.branches[1].get_feature_configs(),
        vec![FeatureConfig {
            feature_id: "feature2".to_string(),
            value: vec![("key".to_string(), json!("value2"))]
                .into_iter()
                .collect()
        }]
    );
    assert!(exp.branches[0].features.is_none());
    assert!(exp.branches[1].features.is_none());
}

#[test]
fn test_feature_and_features_in_one_branch() {
    // Single feature, no features
    let branch: Branch = serde_json::from_value(json!(
        {
            "slug": "control",
            "ratio": 1,
            "feature": {
                "featureId": "feature1",
                "value": {
                    "key": "value"
                }
            }
        }
    ))
    .unwrap();

    let configs = branch.get_feature_configs();

    assert_eq!(
        configs,
        vec![FeatureConfig {
            feature_id: "feature1".to_string(),
            value: json!({"key": "value"}).as_object().unwrap().clone()
        }]
    );

    // No feature, multiple features
    let branch: Branch = serde_json::from_value(json!(
        {
            "slug": "control",
            "ratio": 1,
            "features": [{
                "featureId": "feature1",
                "value": {
                    "key": "value"
                }
            },
            {
                "featureId": "feature2",
                "value": {
                    "key": "value"
                }
            }]
        }
    ))
    .unwrap();

    let configs = branch.get_feature_configs();

    assert_eq!(
        configs,
        vec![
            FeatureConfig {
                feature_id: "feature1".to_string(),
                value: json!({"key": "value"}).as_object().unwrap().clone()
            },
            FeatureConfig {
                feature_id: "feature2".to_string(),
                value: json!({"key": "value"}).as_object().unwrap().clone()
            }
        ]
    );

    // Both feature AND features
    // Some versions of desktop need both, but features are prioritized (https://mozilla-hub.atlassian.net/browse/SDK-440)
    let branch: Branch = serde_json::from_value(json!(
        {
            "slug": "control",
            "ratio": 1,
            "feature": {
                "featureId": "wrong",
                "value": {
                    "key": "value"
                }
            },
            "features": [{
                "featureId": "feature1",
                "value": {
                    "key": "value"
                }
            },
            {
                "featureId": "feature2",
                "value": {
                    "key": "value"
                }
            }]
        }
    ))
    .unwrap();

    let configs = branch.get_feature_configs();

    assert_eq!(
        configs,
        vec![
            FeatureConfig {
                feature_id: "feature1".to_string(),
                value: json!({"key": "value"}).as_object().unwrap().clone()
            },
            FeatureConfig {
                feature_id: "feature2".to_string(),
                value: json!({"key": "value"}).as_object().unwrap().clone()
            }
        ]
    );
}

#[test]
// This was the `Experiment` object schema as it originally shipped to Fenix Nightly.
// It was missing some fields that have since been added.
fn test_experiment_schema_initial_release() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    let exp: Experiment = serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "endDate": null,
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
            },
            {
                "slug": "treatment",
                "ratio":1,
            }
        ],
        "probeSets":[],
        "startDate":null,
        "application":"fenix",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
    assert!(exp.get_feature_ids().is_empty());
}

// In #96 we added a `featureIds` field to the Experiment schema.
// This tests the data as it was after that change.
#[test]
fn test_experiment_schema_with_feature_ids() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    let exp: Experiment = serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "endDate": null,
        "featureIds": ["some_control"],
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": false
                }
            },
            {
                "slug": "treatment",
                "ratio":1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": true
                }
            }
        ],
        "probeSets":[],
        "startDate":null,
        "application":"fenix",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
    assert_eq!(exp.get_feature_ids(), vec!["some_control"]);
}

// In #97 we deprecated `application` and added `app_name`, `app_id`,
// and `channel`.  This tests the ability to deserialize both variants.
#[test]
fn test_experiment_schema_with_adr0004_changes() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️

    // First, test deserializing an `application` format experiment
    // to ensure the presence of `application` doesn't fail.
    let exp: Experiment = serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "endDate": null,
        "featureIds": ["some_control"],
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": false
                }
            },
            {
                "slug": "treatment",
                "ratio":1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": true
                }
            }
        ],
        "probeSets":[],
        "startDate":null,
        "application":"fenix",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
    // Without the fields in the experiment, the resulting fields in the struct
    // should be `None`
    assert_eq!(exp.app_name, None);
    assert_eq!(exp.app_id, None);
    assert_eq!(exp.channel, None);

    // Next, test deserializing an experiment with `app_name`, `app_id`,
    // and `channel`.
    let exp: Experiment = serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "endDate": null,
        "featureIds": ["some_control"],
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": false
                }
            },
            {
                "slug": "treatment",
                "ratio":1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": true
                }
            }
        ],
        "probeSets":[],
        "startDate":null,
        "appName":"fenix",
        "appId":"org.mozilla.fenix",
        "channel":"nightly",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
    assert_eq!(exp.app_name, Some("fenix".to_string()));
    assert_eq!(exp.app_id, Some("org.mozilla.fenix".to_string()));
    assert_eq!(exp.channel, Some("nightly".to_string()));

    // Finally, test deserializing an experiment with `app_name`, `app_id`,
    // `channel` AND `application` to ensure nothing fails.
    let exp: Experiment = serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "endDate": null,
        "featureIds": ["some_control"],
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": false
                }
            },
            {
                "slug": "treatment",
                "ratio":1,
                "feature": {
                    "featureId": "some_control",
                    "enabled": true
                }
            }
        ],
        "probeSets":[],
        "startDate":null,
        "application":"org.mozilla.fenix",
        "appName":"fenix",
        "appId":"org.mozilla.fenix",
        "channel":"nightly",
        "bucketConfig":{
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    }))
    .unwrap();
    assert_eq!(exp.app_name, Some("fenix".to_string()));
    assert_eq!(exp.app_id, Some("org.mozilla.fenix".to_string()));
    assert_eq!(exp.channel, Some("nightly".to_string()));
}
