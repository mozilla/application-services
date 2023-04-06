// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::enrollment::{EnrolledReason, ExperimentEnrollment, NotEnrolledReason};
use crate::{
    tests::test_enrollment::local_ctx, CirrusClient, EnrollmentRequest, EnrollmentStatus, Result,
    TargetingAttributes,
};
use serde_json::{from_str, to_string, to_value, Map, Value};
use uuid::Uuid;

#[test]
fn test_can_instantiate() {
    CirrusClient::new();
}

#[test]
fn test_can_enroll() -> Result<()> {
    let client = CirrusClient::new();
    let (_, context, _) = local_ctx();
    let exp = helpers::get_experiment_with_newtab_feature_branches();

    let result = client.enroll(
        "test".to_string(),
        context.into(),
        true,
        &[],
        &[exp.clone()],
        &[],
    )?;

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
        result.enrollments[0],
        ExperimentEnrollment {
            slug: exp.slug,
            status: EnrollmentStatus::Enrolled {
                enrollment_id: match result.enrollments[0].status {
                    EnrollmentStatus::Enrolled { enrollment_id, .. } => enrollment_id,
                    _ => Uuid::new_v4(),
                },
                reason: EnrolledReason::Qualified,
                branch: "treatment".to_string()
            },
        }
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

    let result = client.enroll(
        "test".to_string(),
        context.into(),
        true,
        &[],
        &[exp],
        &[enrollment],
    )?;

    assert_eq!(result.enrolled_feature_config_map.len(), 0);

    Ok(())
}

#[test]
fn test_handle_enrollment_works_with_request() -> Result<()> {
    let client = CirrusClient::new();
    let (_, context, _) = local_ctx();
    let exp = helpers::get_experiment_with_newtab_feature_branches();

    let request = EnrollmentRequest {
        client_id: Some("test".to_string()),
        context: context.into(),
        next_experiments: vec![exp.clone()],
        ..Default::default()
    };
    let result = client.handle_enrollment(request)?;

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
        result.enrollments[0],
        ExperimentEnrollment {
            slug: exp.slug,
            status: EnrollmentStatus::Enrolled {
                enrollment_id: match result.enrollments[0].status {
                    EnrollmentStatus::Enrolled { enrollment_id, .. } => enrollment_id,
                    _ => Uuid::new_v4(),
                },
                reason: EnrolledReason::Qualified,
                branch: "treatment".to_string()
            },
        }
    );

    Ok(())
}

#[test]
fn test_handle_enrollment_errors_on_no_client_id() -> Result<()> {
    let client = CirrusClient::new();

    let request = EnrollmentRequest {
        client_id: None,
        ..Default::default()
    };
    let result = client.handle_enrollment(request);

    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_handle_enrollment_works_with_json() -> Result<()> {
    let client = CirrusClient::new();
    let (_, context, _) = local_ctx();
    let exp = helpers::get_experiment_with_newtab_feature_branches();
    let context: TargetingAttributes = context.into();

    let request = to_string(&Map::from_iter([
        ("clientId".to_string(), Value::from("test")),
        (
            "context".to_string(),
            Value::Object(from_str(to_string(&context)?.as_str())?),
        ),
        ("isUserParticipating".to_string(), Value::from(true)),
        ("prevExperiments".to_string(), Value::Array(vec![])),
        (
            "nextExperiments".to_string(),
            Value::Array(vec![to_value(exp.clone())?]),
        ),
        ("prevEnrollments".to_string(), Value::Array(vec![])),
    ]))?;

    let result = client.handle_enrollment(from_str(request.as_str())?)?;

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
        result.enrollments[0],
        ExperimentEnrollment {
            slug: exp.slug,
            status: EnrollmentStatus::Enrolled {
                enrollment_id: match result.enrollments[0].status {
                    EnrollmentStatus::Enrolled { enrollment_id, .. } => enrollment_id,
                    _ => Uuid::new_v4(),
                },
                reason: EnrolledReason::Qualified,
                branch: "treatment".to_string()
            },
        }
    );

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
