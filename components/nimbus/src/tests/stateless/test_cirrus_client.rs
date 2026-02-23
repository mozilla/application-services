// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{collections::HashMap, slice};

use serde_json::{Map, Value, from_str, to_string, to_value};

use crate::enrollment::{EnrollmentChangeEventType, ExperimentEnrollment, NotEnrolledReason};
use crate::metrics::EnrollmentStatusExtraDef;
use crate::tests::helpers::TestMetrics;
use crate::tests::stateless::test_cirrus_client::helpers::get_experiment_with_newtab_feature_branches;
use crate::{
    AppContext, CirrusClient, EnrollmentRequest, EnrollmentResponse, EnrollmentStatus, Result,
};

fn create_client() -> Result<CirrusClient> {
    let metrics_handler = TestMetrics::new();
    CirrusClient::new(
        to_string(&AppContext {
            app_id: "test app id".to_string(),
            app_name: "test app name".to_string(),
            channel: "test channel".to_string(),
            app_version: None,
            app_build: None,
            custom_targeting_attributes: None,
        })
        .unwrap(),
        Box::new(metrics_handler),
        Default::default(),
    )
}

#[test]
fn test_can_instantiate() -> Result<()> {
    create_client()?;
    Ok(())
}

#[test]
fn test_can_enroll() -> Result<()> {
    let client = create_client()?;
    let exp = helpers::get_experiment_with_newtab_feature_branches();
    client
        .set_experiments(to_string(&HashMap::from([("data", slice::from_ref(&exp))])).unwrap())
        .unwrap();

    let result = client.enroll("test".to_string(), Default::default(), &[])?;
    assert_eq!(result.enrolled_feature_config_map.len(), 1);
    assert_eq!(
        result
            .enrolled_feature_config_map
            .get("newtab")
            .unwrap()
            .branch
            .clone()
            .unwrap(),
        exp.branches[1].slug
    );
    assert_eq!(
        result.events[0].change,
        EnrollmentChangeEventType::Enrollment
    );

    Ok(())
}

#[test]
fn test_will_not_enroll_if_previously_did_not_enroll() -> Result<()> {
    let client = create_client()?;
    let exp = helpers::get_experiment_with_newtab_feature_branches();
    client
        .set_experiments(to_string(&HashMap::from([("data", slice::from_ref(&exp))])).unwrap())
        .unwrap();

    let enrollment = ExperimentEnrollment {
        slug: exp.slug,
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted,
        },
    };

    let result = client.enroll("test".to_string(), Default::default(), &[enrollment])?;

    assert_eq!(result.events.len(), 0);

    Ok(())
}

#[test]
fn test_handle_enrollment_works_with_json() -> Result<()> {
    let client = create_client()?;
    let exp = helpers::get_experiment_with_newtab_feature_branches_with_targeting(
        "language == 'en' && region == 'US'",
    );
    client
        .set_experiments(to_string(&HashMap::from([("data", slice::from_ref(&exp))])).unwrap())
        .unwrap();

    let request = Map::from_iter(vec![
        ("clientId".to_string(), Value::String("test".to_string())),
        (
            "requestContext".to_string(),
            to_value(Map::from_iter([(
                "locale".to_string(),
                Value::String("en-US".to_string()),
            )]))
            .unwrap(),
        ),
        (
            "nextExperiments".to_string(),
            Value::Array(vec![to_value(exp.clone()).unwrap()]),
        ),
    ]);
    let result: EnrollmentResponse =
        from_str(client.handle_enrollment(to_string(&request)?)?.as_str()).unwrap();

    assert_eq!(result.enrolled_feature_config_map.len(), 1);
    assert_eq!(
        result
            .enrolled_feature_config_map
            .get("newtab")
            .unwrap()
            .branch
            .clone()
            .unwrap(),
        exp.branches[1].slug
    );
    assert_eq!(
        result.events[0].change,
        EnrollmentChangeEventType::Enrollment
    );

    Ok(())
}

#[test]
fn test_handle_enrollment_errors_on_no_client_id() -> Result<()> {
    let client = create_client()?;

    let request = EnrollmentRequest {
        client_id: None,
        ..Default::default()
    };
    let result = client.handle_enrollment(to_string(&request)?);

    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_only_writes_experiments_matching_configured_app_name_and_channel() -> Result<()> {
    let client = create_client()?;

    let mut exp_app_name_mismatch = get_experiment_with_newtab_feature_branches();
    exp_app_name_mismatch.app_name = Some("mismatched name".into());
    let mut exp_channel_mismatch = get_experiment_with_newtab_feature_branches();
    exp_channel_mismatch.channel = Some("mismatched channel".into());

    client.set_experiments(
        to_string(&HashMap::from([(
            "data",
            &[
                exp_app_name_mismatch,
                exp_channel_mismatch,
                get_experiment_with_newtab_feature_branches(),
            ],
        )]))
        .unwrap(),
    )?;

    assert_eq!(client.get_experiments()?.len(), 1);

    Ok(())
}

#[test]
fn test_sends_metrics_on_enrollment() -> Result<()> {
    let metrics_handler = TestMetrics::new();
    let client = CirrusClient::new(
        to_string(&AppContext {
            app_id: "test app id".to_string(),
            app_name: "test app name".to_string(),
            channel: "test channel".to_string(),
            app_version: None,
            app_build: None,
            custom_targeting_attributes: None,
        })
        .unwrap(),
        Box::new(metrics_handler.clone()),
        Default::default(),
    )?;
    let exp = helpers::get_experiment_with_newtab_feature_branches();
    client
        .set_experiments(to_string(&HashMap::from([("data", slice::from_ref(&exp))])).unwrap())
        .unwrap();
    client.enroll("test".to_string(), Default::default(), &[])?;

    let metric_records: Vec<EnrollmentStatusExtraDef> = metrics_handler.get_enrollment_statuses();
    assert_eq!(metric_records.len(), 1);
    assert_eq!(metric_records[0].slug(), exp.slug);
    assert_eq!(metric_records[0].status(), "Enrolled");
    assert_eq!(metric_records[0].reason(), "Qualified");
    assert_eq!(metric_records[0].branch(), "treatment");
    assert_eq!(metric_records[0].user_id(), "test");

    let nimbus_user_id: Option<String> = metrics_handler.get_nimbus_user_id();
    assert_eq!(nimbus_user_id, Some("test".into()));

    Ok(())
}

mod helpers {
    use crate::Experiment;
    use serde_json::json;

    pub fn get_experiment_with_newtab_feature_branches() -> Experiment {
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "newtab-feature-experiment",
            "appId": "test app id",
            "appName": "test app name",
            "channel": "test channel",
            "branches": [
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "newtab",
                        "enabled": false,
                        "value": {},
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "newtab",
                        "enabled": true,
                        "value": {},
                    }
                }
            ],
            "probeSets":[],
            "bucketConfig":{
                // Also enroll everyone.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-silver",
                "randomizationUnit":"user_id"
            },
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"2nd test experiment.",
            "userFacingName":"2nd test experiment",
        }))
        .unwrap()
    }

    pub fn get_experiment_with_newtab_feature_branches_with_targeting(
        targeting: &str,
    ) -> Experiment {
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "newtab-feature-experiment",
            "appId": "test app id",
            "appName": "test app name",
            "channel": "test channel",
            "branches": [
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "newtab",
                        "enabled": false,
                        "value": {},
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "newtab",
                        "enabled": true,
                        "value": {},
                    }
                }
            ],
            "probeSets":[],
            "bucketConfig":{
                // Also enroll everyone.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-silver",
                "randomizationUnit":"user_id"
            },
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"2nd test experiment.",
            "userFacingName":"2nd test experiment",
            "targeting": targeting
        }))
        .unwrap()
    }
}
