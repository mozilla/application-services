// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::enrollment::{EnrollmentChangeEventType, ExperimentEnrollment, NotEnrolledReason};
use crate::matcher::RequestContext;
use crate::{
    tests::test_enrollment::local_ctx, CirrusClient, EnrollmentRequest, EnrollmentResponse,
    EnrollmentStatus, Result, TargetingAttributes,
};
use serde_json::{from_value, to_value, Map, Value};

#[test]
fn test_can_instantiate() {
    CirrusClient::new();
}

#[test]
fn test_can_enroll() -> Result<()> {
    let client = CirrusClient::new();
    let (_, context, _) = local_ctx();
    let exp = helpers::get_experiment_with_newtab_feature_branches();
    let ta = TargetingAttributes::new(context, Default::default());

    let result = client.enroll("test".to_string(), ta, true, &[exp.clone()], &[])?;

    assert_eq!(result.enrolled_feature_config_map.len(), 1);
    assert_eq!(
        result
            .enrolled_feature_config_map
            .get("newtab")
            .unwrap()
            .get("branch")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned(),
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
    let client = CirrusClient::new();
    let (_, context, _) = local_ctx();
    let exp = helpers::get_experiment_with_newtab_feature_branches();
    let enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted,
        },
    };
    let ta = TargetingAttributes::new(context, Default::default());

    let result = client.enroll("test".to_string(), ta, true, &[exp], &[enrollment])?;

    assert_eq!(result.events.len(), 0);

    Ok(())
}

#[test]
fn test_handle_enrollment_works_with_json() -> Result<()> {
    let client = CirrusClient::new();
    let (_, context, _) = local_ctx();
    let exp = helpers::get_experiment_with_newtab_feature_branches();

    let request = Map::from_iter(vec![
        ("clientId".to_string(), Value::String("test".to_string())),
        ("appContext".to_string(), to_value(context).unwrap()),
        (
            "requestContext".to_string(),
            to_value(RequestContext {
                ..Default::default()
            })
            .unwrap(),
        ),
        (
            "nextExperiments".to_string(),
            Value::Array(vec![to_value(exp.clone()).unwrap()]),
        ),
    ]);
    let result: EnrollmentResponse =
        from_value(Value::Object(client.handle_enrollment(request)?)).unwrap();

    assert_eq!(
        result
            .enrolled_feature_config_map
            .get("newtab")
            .unwrap()
            .get("branch")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned(),
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
    let client = CirrusClient::new();

    let request = EnrollmentRequest {
        client_id: None,
        app_context: Map::from_iter(vec![
            ("app_id".to_string(), Value::from("test".to_string())),
            ("app_name".to_string(), Value::from("test".to_string())),
            ("channel".to_string(), Value::from("test".to_string())),
        ]),
        ..Default::default()
    };
    let result = client.handle_enrollment(to_value(request).unwrap().as_object().unwrap().clone());

    assert!(result.is_err());

    Ok(())
}

mod helpers {
    use crate::Experiment;
    use serde_json::json;

    pub fn get_experiment_with_newtab_feature_branches() -> Experiment {
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "newtab-feature-experiment",
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
                "randomizationUnit":"client_id"
            },
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"2nd test experiment.",
            "userFacingName":"2nd test experiment",
        }))
        .unwrap()
    }
}
