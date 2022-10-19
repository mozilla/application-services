/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Testing enrollment.rs

use crate::enrollment::*;
use crate::Experiment;
use crate::{
    defaults::Defaults,
    enrollment::PREVIOUS_ENROLLMENTS_GC_TIME,
    error::Result,
    persistence::{Database, Readable, StoreId},
    AppContext, AvailableRandomizationUnits, Branch, BucketConfig, FeatureConfig,
    TargetingAttributes,
};
use serde_json::{json, Value};
use uuid::Uuid;

use std::collections::{HashMap, HashSet};

fn get_test_experiments() -> Vec<Experiment> {
    vec![
        serde_json::from_value(json!({
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
                        "enabled": false,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        }
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": true,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        }
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
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
        .unwrap(),
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-silver",
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": true,
                    }
                },
                {
                    "slug": "treatment",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": false,
                    }
                },
            ],
            "featureIds": ["about_welcome"],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName":"fenix",
            "appId":"org.mozilla.fenix",
            "bucketConfig":{
                // Also enroll everyone.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-silver",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"2nd test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"2nd test experiment.",
            "id":"secure-silver",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap(),
    ]
}

fn get_feature_conflict_test_experiments() -> Vec<Experiment> {
    vec![
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["about_welcome"],
            "branches": [
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": false,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        },
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": true,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        },
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
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
        .unwrap(),
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-silver",
            "endDate": null,
            "branches": [
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": false,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        },
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": true,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        },
                    }
                }
            ],
            "featureIds": ["about_welcome"],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName":"fenix",
            "appId":"org.mozilla.fenix",
            "bucketConfig":{
                // Also enroll everyone.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-silver",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"2nd test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"2nd test experiment.",
            "id":"secure-silver",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap(),
    ]
}

fn get_experiment_with_newtab_feature_branches() -> Experiment {
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
            "randomizationUnit":"nimbus_id"
        },
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"2nd test experiment.",
        "userFacingName":"2nd test experiment",
    }))
    .unwrap()
}

fn get_experiment_with_different_feature_branches() -> Experiment {
    serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "mixed-feature-experiment",
        "branches": [
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "about_welcome",
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
            "randomizationUnit":"nimbus_id"
        },
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"2nd test experiment.",
        "userFacingName":"2nd test experiment",
    }))
    .unwrap()
}

fn get_experiment_with_different_features_same_branch() -> Experiment {
    serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "multi-feature-experiment",
        "branches": [
            {
                "slug": "control",
                "ratio": 1,
                "features": [{
                    "featureId": "about_welcome",
                    "enabled": false,
                    "value": {},
                },
                {
                    "featureId": "newtab",
                    "enabled": true,
                    "value": {},
                }]
            },
            {
                "slug": "treatment",
                "ratio":1,
                "features": [{
                    "featureId": "onboarding",
                    "enabled": false,
                    "value": {},
                },
                {
                    "featureId": "onboarding",
                    "enabled": true,
                    "value": {},
                }]
            }
        ],
        "probeSets":[],
        "bucketConfig":{
            // Also enroll everyone.
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-silver",
            "randomizationUnit":"nimbus_id"
        },
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"2nd test experiment.",
        "userFacingName":"2nd test experiment",
    }))
    .unwrap()
}

fn get_experiment_with_aboutwelcome_feature_branches() -> Experiment {
    serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "about_welcome-feature-experiment",
        "branches": [
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "about_welcome",
                    "enabled": false,
                    "value": {},
                }
            },
            {
                "slug": "treatment",
                "ratio":1,
                "feature": {
                    "featureId": "about_welcome",
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
            "randomizationUnit":"nimbus_id"
        },
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"2nd test experiment.",
        "userFacingName":"2nd test experiment",
    }))
    .unwrap()
}

fn get_conflicting_experiment() -> Experiment {
    serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "another-monkey",
        "endDate": null,
        "branches":[
            {"slug": "control", "ratio": 1, "feature": { "featureId": "some_control", "enabled": true }},
            {"slug": "treatment","ratio": 1, "feature": { "featureId": "some_control", "enabled": true }},
        ],
        "featureIds": ["some_control"],
        "channel": "nightly",
        "probeSets":[],
        "startDate":null,
        "appName":"fenix",
        "appId":"org.mozilla.fenix",
        "bucketConfig":{
            "count":1_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-silver",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"2nd test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"2nd test experiment.",
        "id":"secure-silver",
        "last_modified":1_602_197_222_372i64
    }))
    .unwrap()
}

fn get_is_already_enrolled_targeting_experiment() -> Experiment {
    serde_json::from_value(json!({
        "schemaVersion": "1.0.0",
        "slug": "another-monkey",
        "endDate": null,
        "branches":[
            {"slug": "control", "ratio": 1, "feature": { "featureId": "some_control", "enabled": true }},
            {"slug": "treatment","ratio": 1, "feature": { "featureId": "some_control", "enabled": true }},
        ],
        "featureIds": ["some_control"],
        "channel": "nightly",
        "probeSets":[],
        "startDate":null,
        "appName":"fenix",
        "appId":"org.mozilla.fenix",
        "bucketConfig":{
            "count":1_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-silver",
            "randomizationUnit":"nimbus_id"
        },
        // We have a logical OR here because we want the user
        // to be enrolled the first time they see this targeting
        // then, we will change the appId in the context to something
        // else, then test enrollment again.
        "targeting": "app_id == 'org.mozilla.fenix' || is_already_enrolled",
        "userFacingName":"2nd test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"2nd test experiment.",
        "id":"secure-silver",
        "last_modified":1_602_197_222_372i64
    }))
    .unwrap()
}

fn get_experiment_enrollments<'r>(
    db: &Database,
    reader: &'r impl Readable<'r>,
) -> Result<Vec<ExperimentEnrollment>> {
    db.get_store(StoreId::Enrollments).collect_all(reader)
}

fn local_ctx() -> (Uuid, AppContext, AvailableRandomizationUnits) {
    // Use a fixed nimbus_id so we don't switch between branches.
    let nimbus_id = Uuid::parse_str("29686b11-00c0-4905-b5e4-f5f945eda60a").unwrap();
    // Create a matching context for the experiments above
    let app_ctx = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let aru = Default::default();
    (nimbus_id, app_ctx, aru)
}

fn enrollment_evolver<'a>(
    nimbus_id: &'a Uuid,
    targeting_attributes: &'a TargetingAttributes,
    aru: &'a AvailableRandomizationUnits,
) -> EnrollmentsEvolver<'a> {
    EnrollmentsEvolver::new(nimbus_id, aru, targeting_attributes)
}

#[test]
fn test_evolver_new_experiment_enrolled() -> Result<()> {
    let exp = &get_test_experiments()[0];
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment(true, None, Some(exp), None, &mut events)?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled { .. }
    ));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].change, EnrollmentChangeEventType::Enrollment);
    Ok(())
}

#[test]
fn test_evolver_new_experiment_not_enrolled() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.bucket_config.count = 0; // Make the experiment bucketing fail.
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment(true, None, Some(&exp), None, &mut events)?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotSelected
        }
    ));
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_new_experiment_globally_opted_out() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment(false, None, Some(&exp), None, &mut events)?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::OptOut
        }
    ));
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_new_experiment_enrollment_paused() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.is_enrollment_paused = true;
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment(true, None, Some(&exp), None, &mut events)?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::EnrollmentsPaused
        }
    ));
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_not_enrolled_opted_out() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::OptOut,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            false,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert_eq!(enrollment.status, existing_enrollment.status);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_not_enrolled_enrollment_paused() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.is_enrollment_paused = true;
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::EnrollmentsPaused,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert_eq!(enrollment.status, existing_enrollment.status);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_not_enrolled_resuming_not_selected() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.bucket_config.count = 0; // Make the experiment bucketing fail.
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::EnrollmentsPaused,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotSelected
        }
    ));
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_not_enrolled_resuming_selected() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::EnrollmentsPaused,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled {
            reason: EnrolledReason::Qualified,
            ..
        }
    ));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].change, EnrollmentChangeEventType::Enrollment);
    Ok(())
}

#[test]
fn test_evolver_experiment_update_enrolled_then_opted_out() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            false,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::OptOut,
            ..
        }
    ));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].enrollment_id, enrollment_id.to_string());
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    assert_eq!(events[0].reason, Some("optout".to_owned()));
    assert_eq!(
        events[0].change,
        EnrollmentChangeEventType::Disqualification
    );
    Ok(())
}

#[test]
fn test_evolver_experiment_update_enrolled_then_experiment_paused() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.is_enrollment_paused = true;
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    dbg!(&enrollment.status);
    if let EnrollmentStatus::Enrolled {
        reason: EnrolledReason::Qualified,
        enrollment_id: new_enrollment_id,
        branch,
        ..
    } = enrollment.status
    {
        assert_eq!(branch, "control");
        assert_eq!(new_enrollment_id, enrollment_id);
    } else {
        panic!("Wrong variant!");
    }
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_enrolled_then_targeting_changed() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, mut app_ctx, aru) = local_ctx();
    app_ctx.app_name = "foobar".to_owned(); // Make the experiment targeting fail.
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    if let EnrollmentStatus::Disqualified {
        reason: DisqualifiedReason::NotTargeted,
        enrollment_id: new_enrollment_id,
        branch,
        ..
    } = enrollment.status
    {
        assert_eq!(branch, "control");
        assert_eq!(new_enrollment_id, enrollment_id);
    } else {
        panic!("Wrong variant! \n{:#?}", enrollment.status);
    }
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].enrollment_id, enrollment_id.to_string());
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    assert_eq!(events[0].reason, Some("targeting".to_owned()));
    assert_eq!(
        events[0].change,
        EnrollmentChangeEventType::Disqualification
    );
    Ok(())
}

#[test]
fn test_evolver_experiment_update_enrolled_then_bucketing_changed() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.bucket_config.count = 0; // Make the experiment bucketing fail.
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert_eq!(enrollment, existing_enrollment);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_enrolled_then_branches_changed() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.branches = vec![
        crate::Branch {
            slug: "control".to_owned(),
            ratio: 0,
            feature: None,
            features: None,
        },
        crate::Branch {
            slug: "bobo-branch".to_owned(),
            ratio: 1,
            feature: None,
            features: None,
        },
    ];
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert_eq!(enrollment, existing_enrollment);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_enrolled_then_branch_disappears() -> Result<()> {
    let mut exp = get_test_experiments()[0].clone();
    exp.branches = vec![crate::Branch {
        slug: "bobo-branch".to_owned(),
        ratio: 1,
        feature: None,
        features: None,
    }];
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::Error,
            ..
        }
    ));
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].change,
        EnrollmentChangeEventType::Disqualification
    );
    assert_eq!(events[0].enrollment_id, enrollment_id.to_string());
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    Ok(())
}

#[test]
fn test_evolver_experiment_update_disqualified_then_opted_out() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Disqualified {
            enrollment_id,
            branch: "control".to_owned(),
            reason: DisqualifiedReason::NotTargeted,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            false,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::OptOut,
            ..
        }
    ));
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_disqualified_then_bucketing_ok() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Disqualified {
            enrollment_id,
            branch: "control".to_owned(),
            reason: DisqualifiedReason::NotTargeted,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert_eq!(enrollment, existing_enrollment);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_feature_can_have_only_one_experiment() -> Result<()> {
    let _ = env_logger::try_init();

    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    // Let's go from no experiments, to some experiments.
    let existing_experiments: Vec<Experiment> = vec![];
    let existing_enrollments: Vec<ExperimentEnrollment> = vec![];
    let updated_experiments = get_feature_conflict_test_experiments();
    let (enrollments, _events) = evolver.evolve_enrollments(
        true,
        &existing_experiments,
        &updated_experiments,
        &existing_enrollments,
    )?;

    assert_eq!(2, enrollments.len());

    let enrolled: Vec<ExperimentEnrollment> = enrollments
        .clone()
        .into_iter()
        .filter(|e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .collect();
    assert_eq!(1, enrolled.len());

    let enrolled1 = enrolled;

    // Test to ensure that features are being de-serialized and copied into EnrolledFeatureConfig and mapped
    // properly to the feature id.
    let features = map_features_by_feature_id(&enrollments, &updated_experiments);
    assert_eq!(features.len(), 1);
    assert!(features.contains_key("about_welcome"));

    let enrolled_feature = features.get("about_welcome").unwrap();
    assert_eq!(
        serde_json::Value::Object(enrolled_feature.feature.value.clone()),
        json!({ "text": "OK then", "number": 42})
    );

    let string = serde_json::to_string(&enrolled_feature.feature.value).unwrap();
    assert_eq!(string, "{\"number\":42,\"text\":\"OK then\"}");

    // Now let's keep the same number of experiments.
    // We should get the same results as before.
    // This time we're testing with a non-empty starting condition.
    let existing_experiments: Vec<Experiment> = updated_experiments;
    let existing_enrollments: Vec<ExperimentEnrollment> = enrollments;
    let updated_experiments = get_feature_conflict_test_experiments();
    let (enrollments, _events) = evolver.evolve_enrollments(
        true,
        &existing_experiments,
        &updated_experiments,
        &existing_enrollments,
    )?;

    assert_eq!(2, enrollments.len());

    let enrolled: Vec<ExperimentEnrollment> = enrollments
        .clone()
        .into_iter()
        .filter(|e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .collect();
    assert_eq!(1, enrolled.len());
    let enrolled2 = enrolled;

    assert_eq!(enrolled1, enrolled2);

    // Let's hold it one more time.
    //
    // XXXdmose I understand why we did this twice, but what's the point
    // of doing it a third time?  To prove idempotency for this set of
    // state transitions?
    let existing_experiments: Vec<Experiment> = updated_experiments;
    let existing_enrollments: Vec<ExperimentEnrollment> = enrollments;
    let updated_experiments = get_feature_conflict_test_experiments();
    let (enrollments, _events) = evolver.evolve_enrollments(
        true,
        &existing_experiments,
        &updated_experiments,
        &existing_enrollments,
    )?;

    assert_eq!(2, enrollments.len());

    let enrolled: Vec<ExperimentEnrollment> = enrollments
        .clone()
        .into_iter()
        .filter(|e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .collect();
    assert_eq!(1, enrolled.len());
    let enrolled3 = enrolled;

    assert_eq!(enrolled2, enrolled3);

    // Ok, no more experiments.
    let existing_experiments: Vec<Experiment> = updated_experiments;
    let existing_enrollments: Vec<ExperimentEnrollment> = enrollments;
    let updated_experiments: Vec<Experiment> = vec![];
    let (enrollments, _events) = evolver.evolve_enrollments(
        true,
        &existing_experiments,
        &updated_experiments,
        &existing_enrollments,
    )?;

    // There should be one WasEnrolled; the NotEnrolled will have been
    // discarded.

    assert_eq!(
        1,
        enrollments
            .clone()
            .into_iter()
            .filter(|e| matches!(e.status, EnrollmentStatus::WasEnrolled { .. }))
            .count()
    );

    assert_eq!(
        0,
        enrollments
            .into_iter()
            .filter(|e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .count()
    );
    Ok(())
}

#[test]
fn test_experiment_get_feature_ids() -> Result<()> {
    let experiment = get_conflicting_experiment();
    assert!(experiment.get_branch("control").is_some());

    let branch = experiment.get_branch("control").unwrap();
    assert_eq!(branch.slug, "control");

    let feature_config = &branch.feature;
    assert!(feature_config.is_some());

    assert_eq!(branch.get_feature_configs().len(), 1);
    assert_eq!(experiment.get_feature_ids(), vec!["some_control"]);

    let experiment = get_experiment_with_different_feature_branches();
    assert_eq!(
        experiment.get_feature_ids().iter().collect::<HashSet<_>>(),
        vec!["newtab".to_string(), "about_welcome".to_string()]
            .iter()
            .collect::<HashSet<_>>()
    );
    Ok(())
}

#[test]
fn test_evolver_experiment_not_enrolled_feature_conflict() -> Result<()> {
    let _ = env_logger::try_init();

    let mut test_experiments = get_test_experiments();
    test_experiments.push(get_conflicting_experiment());
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let (enrollments, events) = evolver.evolve_enrollments(true, &[], &test_experiments, &[])?;

    assert_eq!(
        enrollments.len(),
        3,
        "There should be exactly 3 ExperimentEnrollments returned"
    );

    let not_enrolleds = enrollments
        .iter()
        .filter(|&e| {
            matches!(
                e.status,
                EnrollmentStatus::NotEnrolled {
                    reason: NotEnrolledReason::FeatureConflict
                }
            )
        })
        .count();
    assert_eq!(
        1, not_enrolleds,
        "exactly one enrollment should have NotEnrolled status"
    );

    let enrolled_count = enrollments
        .iter()
        .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .count();
    assert_eq!(
        2, enrolled_count,
        "exactly two enrollments should have Enrolled status"
    );

    log::debug!("events: {:?}", events);

    assert_eq!(
        3,
        events.len(),
        "There should be exactly 3 enrollment_change_events (Enroll/Enroll/EnrollFailed)"
    );

    let enrolled_events = events
        .iter()
        .filter(|&e| matches!(e.change, EnrollmentChangeEventType::Enrollment))
        .count();
    assert_eq!(
        2, enrolled_events,
        "exactly two events should have Enrolled event types"
    );

    Ok(())
}

#[test]
fn test_multi_feature_per_branch_conflict() -> Result<()> {
    let mut test_experiments = get_test_experiments();
    test_experiments.push(get_experiment_with_different_features_same_branch());
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let (enrollments, events) = evolver.evolve_enrollments(true, &[], &test_experiments, &[])?;

    let enrolled_count = enrollments
        .iter()
        .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .count();
    // Two of the three experiments conflict with each other
    // other, so only one will be enrolled
    assert_eq!(
        enrolled_count, 2,
        "There should be exactly 2 ExperimentEnrollments returned"
    );

    assert_eq!(
        3,
        events.len(),
        "There should be exactly 3 enrollment_change_events (Enroll/Enroll/EnrollFailed)"
    );

    let enrolled_events = events
        .iter()
        .filter(|&e| matches!(e.change, EnrollmentChangeEventType::Enrollment))
        .count();
    assert_eq!(
        2, enrolled_events,
        "exactly two events should have Enrolled event types"
    );
    Ok(())
}

#[test]
fn test_evolver_feature_id_reuse() -> Result<()> {
    let _ = env_logger::try_init();

    let test_experiments = get_test_experiments();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let (enrollments, _) = evolver.evolve_enrollments(true, &[], &test_experiments, &[])?;

    let enrolled_count = enrollments
        .iter()
        .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .count();
    assert_eq!(
        2, enrolled_count,
        "exactly two enrollments should have Enrolled status"
    );

    let conflicting_experiment = get_conflicting_experiment();
    let (enrollments, events) = evolver.evolve_enrollments(
        true,
        &test_experiments,
        &[test_experiments[1].clone(), conflicting_experiment.clone()],
        &enrollments,
    )?;

    log::debug!("events = {:?}", events);

    assert_eq!(events.len(), 2);

    // we didn't include test_experiments[1] in next_experiments above,
    // so it should have been unenrolled...
    assert_eq!(events[0].experiment_slug, test_experiments[0].slug);
    assert_eq!(events[0].change, EnrollmentChangeEventType::Unenrollment);

    // ...which will have gotten rid of the thing that otherwise would have
    // conflicted with conflicting_experiment, allowing it to have now
    // been enrolled.
    assert_eq!(events[1].experiment_slug, conflicting_experiment.slug);
    assert_eq!(events[1].change, EnrollmentChangeEventType::Enrollment);

    let enrolled_count = enrollments
        .iter()
        .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .count();
    assert_eq!(
        2, enrolled_count,
        "exactly two enrollments should have Enrolled status"
    );

    Ok(())
}

#[test]
fn test_evolver_multi_feature_experiments() -> Result<()> {
    let _ = env_logger::try_init();

    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);

    let aboutwelcome_experiment = get_experiment_with_aboutwelcome_feature_branches();
    let newtab_experiment = get_experiment_with_newtab_feature_branches();
    let mixed_experiment = get_experiment_with_different_feature_branches();
    let multi_feature_experiment = get_experiment_with_different_features_same_branch();
    // 1. we have two experiments that use one feature each. There's no conflicts.
    let next_experiments = vec![aboutwelcome_experiment.clone(), newtab_experiment.clone()];

    let (enrollments, _) = evolver.evolve_enrollments(true, &[], &next_experiments, &[])?;

    let feature_map = map_features_by_feature_id(&enrollments, &next_experiments);
    assert_eq!(feature_map.len(), 2);
    assert_eq!(
        feature_map.get("about_welcome").unwrap().slug,
        "about_welcome-feature-experiment"
    );
    assert_eq!(
        feature_map.get("newtab").unwrap().slug,
        "newtab-feature-experiment"
    );

    assert_eq!(
        enrollments
            .iter()
            .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .map(|e| e.slug.clone())
            .collect::<HashSet<_>>(),
        vec![
            "newtab-feature-experiment",
            "about_welcome-feature-experiment"
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<HashSet<_>>()
    );

    // 2. We add a third, which uses both the features that the other experiments use, i.e. it shouldn't be enrolled.
    let prev_enrollments = enrollments;
    let prev_experiments = next_experiments;
    let next_experiments = vec![
        aboutwelcome_experiment.clone(),
        newtab_experiment.clone(),
        mixed_experiment.clone(),
    ];
    let (enrollments, events) = evolver.evolve_enrollments(
        true,
        &prev_experiments,
        &next_experiments,
        &prev_enrollments,
    )?;

    assert_eq!(
        events.len(),
        1,
        "A single EnrollFailed recorded due to the feature-conflict"
    );

    let feature_map = map_features_by_feature_id(&enrollments, &next_experiments);
    assert_eq!(feature_map.len(), 2);
    assert_eq!(
        feature_map.get("about_welcome").unwrap().slug,
        "about_welcome-feature-experiment"
    );
    assert_eq!(
        feature_map.get("newtab").unwrap().slug,
        "newtab-feature-experiment"
    );

    assert_eq!(
        enrollments
            .iter()
            .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .map(|e| e.slug.clone())
            .collect::<HashSet<_>>(),
        vec![
            "newtab-feature-experiment",
            "about_welcome-feature-experiment"
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<HashSet<_>>()
    );

    // 3. Next we take away each of the single feature experiments, until the multi-feature can enroll.
    let prev_enrollments = enrollments;
    let prev_experiments = next_experiments;
    let next_experiments = vec![newtab_experiment.clone(), mixed_experiment.clone()];
    let (enrollments, _) = evolver.evolve_enrollments(
        true,
        &prev_experiments,
        &next_experiments,
        &prev_enrollments,
    )?;

    let feature_map = map_features_by_feature_id(&enrollments, &next_experiments);
    assert_eq!(feature_map.len(), 1);
    assert!(feature_map.get("about_welcome").is_none());
    assert_eq!(
        feature_map.get("newtab").unwrap().slug,
        "newtab-feature-experiment"
    );

    assert_eq!(
        enrollments
            .iter()
            .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .map(|e| e.slug.clone())
            .collect::<HashSet<_>>(),
        vec!["newtab-feature-experiment"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
    );

    // 3a. Take away the second single-feature experiment. The multi-feature one now can enroll.
    let prev_enrollments = enrollments;
    let prev_experiments = next_experiments;
    let next_experiments = vec![mixed_experiment.clone()];
    let (enrollments, _) = evolver.evolve_enrollments(
        true,
        &prev_experiments,
        &next_experiments,
        &prev_enrollments,
    )?;

    let feature_map = map_features_by_feature_id(&enrollments, &next_experiments);
    assert_eq!(feature_map.len(), 2);
    assert_eq!(
        feature_map.get("about_welcome").unwrap().slug,
        "mixed-feature-experiment"
    );
    assert_eq!(
        feature_map.get("newtab").unwrap().slug,
        "mixed-feature-experiment"
    );

    assert_eq!(
        enrollments
            .iter()
            .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .map(|e| e.slug.clone())
            .collect::<HashSet<_>>(),
        vec!["mixed-feature-experiment"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
    );

    // 4. Starting from an empty enrollments, enroll a multi-feature and then add the single feature ones back in again, which won't be able to enroll.
    // 4a. The multi feature experiment.
    let prev_enrollments = vec![];
    let prev_experiments = vec![];
    let next_experiments = vec![mixed_experiment.clone()];
    let (enrollments, _) = evolver.evolve_enrollments(
        true,
        &prev_experiments,
        &next_experiments,
        &prev_enrollments,
    )?;

    // 4b. Add the single feature experiments.
    let prev_enrollments = enrollments;
    let prev_experiments = next_experiments;
    let next_experiments = vec![
        aboutwelcome_experiment,
        newtab_experiment,
        mixed_experiment.clone(),
    ];
    let (enrollments, events) = evolver.evolve_enrollments(
        true,
        &prev_experiments,
        &next_experiments,
        &prev_enrollments,
    )?;

    assert_eq!(
        events.len(),
        2,
        "Exactly two EnrollFailed events should be recorded"
    );
    let feature_map = map_features_by_feature_id(&enrollments, &next_experiments);
    assert_eq!(feature_map.len(), 2);
    assert_eq!(
        feature_map.get("about_welcome").unwrap().slug,
        "mixed-feature-experiment"
    );
    assert_eq!(
        feature_map.get("newtab").unwrap().slug,
        "mixed-feature-experiment"
    );

    assert_eq!(
        enrollments
            .iter()
            .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .map(|e| e.slug.clone())
            .collect::<HashSet<_>>(),
        vec!["mixed-feature-experiment"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
    );

    // 5. Add in experiment with conflicting features on the **same branch**
    // should not be enrolled!
    let prev_enrollments = enrollments;
    let prev_experiments = next_experiments;
    let next_experiments = vec![mixed_experiment, multi_feature_experiment.clone()];
    let (enrollments, events) = evolver.evolve_enrollments(
        true,
        &prev_experiments,
        &next_experiments,
        &prev_enrollments,
    )?;

    assert_eq!(
        events.len(),
        1,
        "Exactly one EnrollFailed event should be recorded"
    );
    let feature_map = map_features_by_feature_id(&enrollments, &next_experiments);
    assert_eq!(feature_map.len(), 2);
    assert_eq!(
        feature_map.get("about_welcome").unwrap().slug,
        "mixed-feature-experiment"
    );
    assert_eq!(
        feature_map.get("newtab").unwrap().slug,
        "mixed-feature-experiment"
    );

    assert_eq!(
        enrollments
            .iter()
            .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .map(|e| e.slug.clone())
            .collect::<HashSet<_>>(),
        vec!["mixed-feature-experiment"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
    );

    // 6. Now we remove the mixed experiment, and we should get enrolled
    let prev_enrollments = enrollments;
    let prev_experiments = next_experiments;
    let next_experiments = vec![multi_feature_experiment];
    let (enrollments, _) = evolver.evolve_enrollments(
        true,
        &prev_experiments,
        &next_experiments,
        &prev_enrollments,
    )?;

    let feature_map = map_features_by_feature_id(&enrollments, &next_experiments);
    assert_eq!(feature_map.len(), 3);
    assert_eq!(
        feature_map.get("about_welcome").unwrap().slug,
        "multi-feature-experiment"
    );
    assert_eq!(
        feature_map.get("newtab").unwrap().slug,
        "multi-feature-experiment"
    );

    assert_eq!(
        feature_map.get("onboarding").unwrap().slug,
        "multi-feature-experiment"
    );

    assert_eq!(
        enrollments
            .iter()
            .filter(|&e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .map(|e| e.slug.clone())
            .collect::<HashSet<_>>(),
        vec!["multi-feature-experiment"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
    );

    Ok(())
}

#[test]
fn test_evolver_experiment_update_was_enrolled() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::WasEnrolled {
            enrollment_id,
            branch: "control".to_owned(),
            experiment_ended_at: now_secs(),
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert_eq!(enrollment, existing_enrollment);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolve_enrollments_error_handling() -> Result<()> {
    let existing_enrollments = vec![ExperimentEnrollment {
        slug: "secure-gold".to_owned(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id: Uuid::new_v4(),
            branch: "hello".to_owned(), // XXX this OK?
            reason: EnrolledReason::Qualified,
        },
    }];

    let _ = env_logger::try_init();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);

    // test that evolve_enrollments correctly handles the case where a
    // record without a previous enrollment gets dropped
    let test_experiments = get_test_experiments();

    // this should not return an error
    let (enrollments, events) =
        evolver.evolve_enrollments(true, &test_experiments, &test_experiments, &[])?;

    assert_eq!(
        enrollments.len(),
        0,
        "no new enrollments should have been returned"
    );

    assert_eq!(
        events.len(),
        0,
        "no new enrollments should have been returned"
    );

    // Test that evolve_enrollments correctly handles the case where a
    // record with a previous enrollment gets dropped
    let (enrollments, events) =
        evolver.evolve_enrollments(true, &[], &test_experiments, &existing_enrollments[..])?;

    assert_eq!(
        enrollments.len(),
        1,
        "only 1 of 2 enrollments should have been returned, since one caused evolve_enrollment to err"
    );

    assert_eq!(
        events.len(),
        1,
        "only 1 of 2 enrollment events should have been returned, since one caused evolve_enrollment to err"
    );

    Ok(())
}

#[test]
fn test_evolve_enrollments_is_already_enrolled_targeting() -> Result<()> {
    let _ = env_logger::try_init();
    let (nimbus_id, mut app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.clone().into();
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);

    // The targeting for this experiment is
    // "app_id == 'org.mozilla.fenix' || is_already_enrolled"
    let test_experiment = get_is_already_enrolled_targeting_experiment();
    let test_experiments = &[test_experiment];
    // The user should get enrolled, since the targeting is OR'ing the app_id == 'org.mozilla.fenix'
    // and the 'is_already_enrolled'
    let (enrollments, events) = evolver.evolve_enrollments(true, &[], test_experiments, &[])?;
    assert_eq!(
        enrollments.len(),
        1,
        "One enrollment should have been returned"
    );

    assert_eq!(events.len(), 1, "One event should have been returned");

    // we change the app_id so the targeting will only target
    // against the `is_already_enrolled`
    app_ctx.app_id = "org.mozilla.bobo".into();
    let targeting_attributes = app_ctx.into();
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);

    // The user should still be enrolled, since the targeting is OR'ing the app_id == 'org.mozilla.fenix'
    // and the 'is_already_enrolled'
    let (enrollments, events) =
        evolver.evolve_enrollments(true, test_experiments, test_experiments, &enrollments)?;
    assert_eq!(
        enrollments.len(),
        1,
        "The previous enrollment should have been evolved"
    );

    assert_eq!(
        events.len(),
        0,
        "no new events should have been returned, the user was already enrolled"
    );
    Ok(())
}

#[test]
fn test_evolver_experiment_update_error() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Error {
            reason: "heh".to_owned(),
        },
    };
    // We should attempt to enroll even though we errored out!
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled { .. }
    ));
    assert_eq!(events.len(), 1);
    Ok(())
}

#[test]
fn test_evolver_experiment_ended_was_enrolled() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            None,
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    if let EnrollmentStatus::WasEnrolled {
        branch,
        enrollment_id: new_enrollment_id,
        ..
    } = enrollment.status
    {
        assert_eq!(branch, "control");
        assert_eq!(new_enrollment_id, enrollment_id);
    } else {
        panic!("Wrong variant!");
    }
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].enrollment_id, enrollment_id.to_string());
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    assert_eq!(events[0].change, EnrollmentChangeEventType::Unenrollment);
    Ok(())
}

#[test]
fn test_evolver_experiment_ended_was_disqualified() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Disqualified {
            enrollment_id,
            branch: "control".to_owned(),
            reason: DisqualifiedReason::NotTargeted,
        },
    };
    let enrollment = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            None,
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    if let EnrollmentStatus::WasEnrolled {
        branch,
        enrollment_id: new_enrollment_id,
        ..
    } = enrollment.status
    {
        assert_eq!(branch, "control");
        assert_eq!(new_enrollment_id, enrollment_id);
    } else {
        panic!("Wrong variant!");
    }
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].enrollment_id, enrollment_id.to_string());
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    assert_eq!(events[0].change, EnrollmentChangeEventType::Unenrollment);
    Ok(())
}

#[test]
fn test_evolver_experiment_ended_was_not_enrolled() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted,
        },
    };
    let enrollment = evolver.evolve_enrollment(
        true,
        Some(&exp),
        None,
        Some(&existing_enrollment),
        &mut events,
    )?;
    assert!(enrollment.is_none());
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_garbage_collection_before_threshold() -> Result<()> {
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: "secure-gold".to_owned(),
        status: EnrollmentStatus::WasEnrolled {
            enrollment_id: Uuid::new_v4(),
            branch: "control".to_owned(),
            experiment_ended_at: now_secs(),
        },
    };
    let enrollment =
        evolver.evolve_enrollment(true, None, None, Some(&existing_enrollment), &mut events)?;
    assert_eq!(enrollment.unwrap(), existing_enrollment);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_garbage_collection_after_threshold() -> Result<()> {
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: "secure-gold".to_owned(),
        status: EnrollmentStatus::WasEnrolled {
            enrollment_id: Uuid::new_v4(),
            branch: "control".to_owned(),
            experiment_ended_at: now_secs() - PREVIOUS_ENROLLMENTS_GC_TIME.as_secs() - 60,
        },
    };
    let enrollment =
        evolver.evolve_enrollment(true, None, None, Some(&existing_enrollment), &mut events)?;
    assert!(enrollment.is_none());
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_new_experiment_enrollment_already_exists() {
    let exp = get_test_experiments()[0].clone();
    let existing_enrollment = ExperimentEnrollment {
        slug: "secure-gold".to_owned(),
        status: EnrollmentStatus::WasEnrolled {
            enrollment_id: Uuid::new_v4(),
            branch: "control".to_owned(),
            experiment_ended_at: now_secs(),
        },
    };
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let res = evolver.evolve_enrollment(
        true,
        None,
        Some(&exp),
        Some(&existing_enrollment),
        &mut vec![],
    );
    assert!(res.is_err());
}

#[test]
fn test_evolver_existing_experiment_has_no_enrollment() {
    let exp = get_test_experiments()[0].clone();
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let res = evolver.evolve_enrollment(true, Some(&exp), Some(&exp), None, &mut vec![]);
    assert!(res.is_err());
}

#[test]
#[should_panic]
fn test_evolver_no_experiments_no_enrollment() {
    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    evolver
        .evolve_enrollment(true, None, None, None, &mut vec![])
        .unwrap();
}

#[test]
fn test_evolver_rollouts_do_not_conflict_with_experiments() -> Result<()> {
    let exp_slug = "experiment1".to_string();
    let experiment = Experiment {
        slug: exp_slug.clone(),
        is_rollout: false,
        branches: vec![Branch {
            features: Some(vec![
                FeatureConfig {
                    feature_id: "alice".into(),
                    ..Default::default()
                },
                FeatureConfig {
                    feature_id: "bob".into(),
                    ..Default::default()
                },
            ]),
            ratio: 1,
            ..Default::default()
        }],
        bucket_config: BucketConfig::always(),
        ..Default::default()
    };

    let ro_slug = "rollout1".to_string();
    let rollout = Experiment {
        slug: ro_slug.clone(),
        is_rollout: true,
        ..experiment.clone()
    };

    let recipes = &[experiment, rollout];

    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let (enrollments, events) = evolver.evolve_enrollments(true, &[], recipes, &[])?;
    assert_eq!(enrollments.len(), 2);
    assert_eq!(events.len(), 2);

    assert_eq!(
        enrollments
            .iter()
            .filter(|e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .count(),
        2
    );

    let slugs: Vec<String> = enrollments.iter().map(|e| e.slug.clone()).collect();
    assert_eq!(slugs, vec![ro_slug, exp_slug]);
    Ok(())
}

#[test]
fn test_evolver_rollouts_do_not_conflict_with_rollouts() -> Result<()> {
    let exp_slug = "experiment1".to_string();
    let experiment = Experiment {
        slug: exp_slug.clone(),
        is_rollout: false,
        branches: vec![Branch {
            features: Some(vec![
                FeatureConfig {
                    feature_id: "alice".into(),
                    ..Default::default()
                },
                FeatureConfig {
                    feature_id: "bob".into(),
                    ..Default::default()
                },
            ]),
            ratio: 1,
            ..Default::default()
        }],
        bucket_config: BucketConfig::always(),
        ..Default::default()
    };

    let ro_slug = "rollout1".to_string();
    let rollout = Experiment {
        slug: ro_slug.clone(),
        is_rollout: true,
        ..experiment.clone()
    };

    let ro_slug2 = "rollout2".to_string();
    let rollout2 = Experiment {
        slug: ro_slug2.clone(),
        ..rollout.clone()
    };

    let recipes = &[experiment, rollout, rollout2];

    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);
    let (enrollments, events) = evolver.evolve_enrollments(true, &[], recipes, &[])?;
    assert_eq!(enrollments.len(), 3);
    assert_eq!(events.len(), 3);

    let enrollments: Vec<ExperimentEnrollment> = enrollments
        .into_iter()
        .filter(|e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
        .collect();

    assert_eq!(enrollments.len(), 2);

    let slugs: HashSet<String> = enrollments.iter().map(|e| e.slug.clone()).collect();
    assert!(slugs.contains(&exp_slug));
    // we want one rollout slug, or the other, but not both, and not none.
    assert!(slugs.contains(&ro_slug) ^ slugs.contains(&ro_slug2));
    Ok(())
}

#[test]
fn test_defaults_merging_feature_configs() -> Result<()> {
    let exp_bob = FeatureConfig {
        feature_id: "bob".into(),
        value: json!({
            "specified": "Experiment in part".to_string(),
        })
        .as_object()
        .unwrap()
        .to_owned(),
    };
    let ro_bob = FeatureConfig {
        feature_id: "bob".into(),
        value: json!({
            "name": "Bob".to_string(),
            "specified": "Rollout".to_string(),
        })
        .as_object()
        .unwrap()
        .to_owned(),
    };

    let bob = exp_bob.defaults(&ro_bob)?;
    assert_eq!(bob.feature_id, "bob".to_string());

    assert_eq!(
        Value::Object(bob.value),
        json!({
            "name": "Bob".to_string(),
            "specified": "Experiment in part".to_string(),
        })
    );

    let exp_bob = EnrolledFeatureConfig {
        feature: exp_bob.clone(),
        slug: "exp".to_string(),
        branch: Some("treatment".to_string()),
        feature_id: exp_bob.feature_id,
    };

    let ro_bob = EnrolledFeatureConfig {
        feature: ro_bob,
        slug: "ro".to_string(),
        branch: None,
        feature_id: exp_bob.feature_id.clone(),
    };

    let bob = exp_bob.defaults(&ro_bob)?.feature;
    assert_eq!(bob.feature_id, "bob".to_string());

    assert_eq!(
        Value::Object(bob.value),
        json!({
            "name": "Bob".to_string(),
            "specified": "Experiment in part".to_string(),
        })
    );

    Ok(())
}

fn get_rollout_and_experiment() -> (Experiment, Experiment) {
    let exp_slug = "experiment1".to_string();
    let experiment = Experiment {
        slug: exp_slug.clone(),
        is_rollout: false,
        branches: vec![Branch {
            slug: exp_slug,
            features: Some(vec![
                FeatureConfig {
                    feature_id: "alice".into(),
                    value: json!({
                        "name": "Alice".to_string(),
                        "specified": "Experiment only".to_string(),
                    })
                    .as_object()
                    .unwrap()
                    .to_owned(),
                },
                FeatureConfig {
                    feature_id: "bob".into(),
                    value: json!({
                        "specified": "Experiment in part".to_string(),
                    })
                    .as_object()
                    .unwrap()
                    .to_owned(),
                },
            ]),
            ratio: 1,
            ..Default::default()
        }],
        bucket_config: BucketConfig::always(),
        ..Default::default()
    };

    let ro_slug = "rollout1".to_string();
    let rollout = Experiment {
        slug: ro_slug.clone(),
        is_rollout: true,
        branches: vec![Branch {
            slug: ro_slug,
            features: Some(vec![
                FeatureConfig {
                    feature_id: "bob".into(),
                    value: json!({
                        "name": "Bob".to_string(),
                        "specified": "Rollout".to_string(),
                    })
                    .as_object()
                    .unwrap()
                    .to_owned(),
                },
                FeatureConfig {
                    feature_id: "charlie".into(),
                    value: json!({
                        "name": "Charlie".to_string(),
                        "specified": "Rollout".to_string(),
                    })
                    .as_object()
                    .unwrap()
                    .to_owned(),
                },
            ]),
            ratio: 1,
            ..Default::default()
        }],
        bucket_config: BucketConfig::always(),
        ..Default::default()
    };

    (rollout, experiment)
}

fn assert_alice_bob_charlie(features: &HashMap<String, EnrolledFeatureConfig>) {
    assert_eq!(features.len(), 3);

    let alice = &features["alice"];
    let bob = &features["bob"];
    let charlie = &features["charlie"];

    assert!(!alice.is_rollout());
    assert_eq!(
        Value::Object(alice.feature.value.clone()),
        json!({
            "name": "Alice".to_string(),
            "specified": "Experiment only".to_string(),
        })
    );

    assert!(!bob.is_rollout());
    assert_eq!(
        Value::Object(bob.feature.value.clone()),
        json!({
            "name": "Bob".to_string(),
            "specified": "Experiment in part".to_string(),
        })
    );

    assert!(charlie.is_rollout());
    assert_eq!(
        Value::Object(charlie.feature.value.clone()),
        json!({
            "name": "Charlie".to_string(),
            "specified": "Rollout".to_string(),
        })
    );
}

#[test]
fn test_evolver_map_features_by_feature_id_merges_rollouts() -> Result<()> {
    let (rollout, experiment) = get_rollout_and_experiment();
    let (exp_slug, ro_slug) = (rollout.slug.clone(), experiment.slug.clone());

    let exp_enrollment = ExperimentEnrollment {
        slug: exp_slug.clone(),
        status: EnrollmentStatus::Enrolled {
            branch: exp_slug,
            enrollment_id: Default::default(),
            reason: EnrolledReason::Qualified,
        },
    };

    let ro_enrollment = ExperimentEnrollment {
        slug: ro_slug.clone(),
        status: EnrollmentStatus::Enrolled {
            branch: ro_slug,
            enrollment_id: Default::default(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollments = &[ro_enrollment, exp_enrollment];
    let experiments = &[experiment, rollout];
    let features = map_features_by_feature_id(enrollments, experiments);

    assert_alice_bob_charlie(&features);
    Ok(())
}

#[test]
fn test_rollouts_end_to_end() -> Result<()> {
    let (rollout, experiment) = get_rollout_and_experiment();
    let recipes = &[rollout, experiment];

    let (nimbus_id, app_ctx, aru) = local_ctx();
    let targeting_attributes = app_ctx.into();
    let evolver = enrollment_evolver(&nimbus_id, &targeting_attributes, &aru);

    let (enrollments, _events) = evolver.evolve_enrollments(true, &[], recipes, &[])?;

    let features = map_features_by_feature_id(&enrollments, recipes);

    assert_alice_bob_charlie(&features);

    Ok(())
}

#[test]
fn test_enrollment_explicit_opt_in() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let mut events = vec![];
    let enrollment = ExperimentEnrollment::from_explicit_opt_in(&exp, "control", &mut events)?;
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled {
            reason: EnrolledReason::OptIn,
            ..
        }
    ));
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].change,
        EnrollmentChangeEventType::Enrollment
    ));
    Ok(())
}

#[test]
fn test_enrollment_explicit_opt_in_branch_unknown() {
    let exp = get_test_experiments()[0].clone();
    let mut events = vec![];
    let res = ExperimentEnrollment::from_explicit_opt_in(&exp, "bobo", &mut events);
    assert!(res.is_err());
}

#[test]
fn test_enrollment_enrolled_explicit_opt_out() {
    let exp = get_test_experiments()[0].clone();
    let mut events = vec![];
    let enrollment_id = Uuid::new_v4();
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug,
        status: EnrollmentStatus::Enrolled {
            enrollment_id,
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = existing_enrollment.on_explicit_opt_out(&mut events);
    if let EnrollmentStatus::Disqualified {
        enrollment_id: new_enrollment_id,
        branch,
        ..
    } = enrollment.status
    {
        assert_eq!(branch, "control");
        assert_eq!(new_enrollment_id, enrollment_id);
    } else {
        panic!("Wrong variant!");
    }
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].change,
        EnrollmentChangeEventType::Disqualification
    ));
}

#[test]
fn test_enrollment_not_enrolled_explicit_opt_out() {
    let exp = get_test_experiments()[0].clone();
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug,
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted,
        },
    };
    let enrollment = existing_enrollment.on_explicit_opt_out(&mut events);
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::OptOut,
            ..
        }
    ));
    assert!(events.is_empty());
}

#[test]
fn test_enrollment_disqualified_explicit_opt_out() {
    let exp = get_test_experiments()[0].clone();
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug,
        status: EnrollmentStatus::Disqualified {
            enrollment_id: Uuid::new_v4(),
            branch: "control".to_owned(),
            reason: DisqualifiedReason::NotTargeted,
        },
    };
    let enrollment = existing_enrollment.on_explicit_opt_out(&mut events);
    assert_eq!(enrollment, existing_enrollment);
    assert!(events.is_empty());
}

// Older tests that also use the DB.
// XXX: make them less complicated (since the transitions are covered above), just see if we write to the DB properly.

#[test]
fn test_enrollments() -> Result<()> {
    let _ = env_logger::try_init();
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;
    let exp1 = get_test_experiments()[0].clone();
    let nimbus_id = Uuid::new_v4();
    let aru = Default::default();
    let targeting_attributes = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();
    assert_eq!(get_enrollments(&db, &writer)?.len(), 0);

    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &[exp1])?;

    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 1);
    let enrollment = &enrollments[0];
    assert_eq!(enrollment.slug, "secure-gold");
    assert_eq!(enrollment.user_facing_name, "Diagnostic test experiment");
    assert_eq!(
        enrollment.user_facing_description,
        "This is a test experiment for diagnostic purposes."
    );
    assert!(enrollment.branch_slug == "control" || enrollment.branch_slug == "treatment");
    // Ensure the event was created too.
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.experiment_slug, "secure-gold");
    assert!(event.branch_slug == "control" || event.branch_slug == "treatment");
    assert!(matches!(
        event.change,
        EnrollmentChangeEventType::Enrollment
    ));

    // Get the ExperimentEnrollment from the DB.
    let ee: ExperimentEnrollment = db
        .get_store(StoreId::Enrollments)
        .get(&writer, "secure-gold")?
        .expect("should exist");
    assert!(matches!(
        ee.status,
        EnrollmentStatus::Enrolled {
            reason: EnrolledReason::Qualified,
            ..
        }
    ));

    // Now opt-out.
    opt_out(&db, &mut writer, "secure-gold")?;
    assert_eq!(get_enrollments(&db, &writer)?.len(), 0);
    // check we recorded the "why" correctly.
    let ee: ExperimentEnrollment = db
        .get_store(StoreId::Enrollments)
        .get(&writer, "secure-gold")?
        .expect("should exist");
    assert!(matches!(
        ee.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::OptOut,
            ..
        }
    ));

    // Opt in to a specific branch.
    opt_in_with_branch(&db, &mut writer, "secure-gold", "treatment")?;
    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 1);
    let enrollment = &enrollments[0];
    assert_eq!(enrollment.slug, "secure-gold");
    assert!(enrollment.branch_slug == "treatment");

    writer.commit()?;
    Ok(())
}

#[test]
fn test_updates() -> Result<()> {
    let _ = env_logger::try_init();
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;
    let nimbus_id = Uuid::new_v4();
    let aru = Default::default();
    let targeting_attributes = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();
    assert_eq!(get_enrollments(&db, &writer)?.len(), 0);
    let exps = get_test_experiments();

    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 2);
    assert_eq!(events.len(), 2);

    // pretend we just updated from the server and one of the 2 is missing.
    let exps = &[exps[1].clone()];
    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, exps)?;

    // should only have 1 now.
    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 1);
    // Check that the un-enrolled event was emitted.
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.experiment_slug, "secure-gold");
    assert!(matches!(
        event.change,
        EnrollmentChangeEventType::Unenrollment
    ));

    writer.commit()?;
    Ok(())
}

#[test]
fn test_global_opt_out() -> Result<()> {
    let _ = env_logger::try_init();
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;
    let nimbus_id = Uuid::new_v4();
    let targeting_attributes = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();
    let aru = Default::default();
    assert_eq!(get_enrollments(&db, &writer)?.len(), 0);
    let exps = get_test_experiments();

    // User has opted out of new experiments.
    set_global_user_participation(&db, &mut writer, false)?;

    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 0);
    assert!(events.is_empty());
    // We should see the experiment non-enrollments.
    assert_eq!(get_experiment_enrollments(&db, &writer)?.len(), 2);
    let num_not_enrolled_enrollments = get_experiment_enrollments(&db, &writer)?
        .into_iter()
        .filter(|enr| {
            matches!(
                enr.status,
                EnrollmentStatus::NotEnrolled {
                    reason: NotEnrolledReason::OptOut
                }
            )
        })
        .count();
    assert_eq!(num_not_enrolled_enrollments, 2);

    // User opts in, and updating should enroll us in 2 experiments.
    set_global_user_participation(&db, &mut writer, true)?;

    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 2);
    assert_eq!(events.len(), 2);
    // We should see 2 experiment enrollments.
    assert_eq!(get_experiment_enrollments(&db, &writer)?.len(), 2);
    let num_enrolled_enrollments = get_experiment_enrollments(&db, &writer)?
        .into_iter()
        .filter(|enr| matches!(enr.status, EnrollmentStatus::Enrolled { .. }))
        .count();
    assert_eq!(num_enrolled_enrollments, 2);

    // Opting out and updating should give us two disqualified enrollments
    set_global_user_participation(&db, &mut writer, false)?;

    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 0);
    assert_eq!(events.len(), 2);
    // We should see 2 experiment enrolments, this time they're both opt outs
    assert_eq!(get_experiment_enrollments(&db, &writer)?.len(), 2);

    assert_eq!(
        get_experiment_enrollments(&db, &writer)?
            .into_iter()
            .filter(|enr| {
                matches!(
                    enr.status,
                    EnrollmentStatus::Disqualified {
                        reason: DisqualifiedReason::OptOut,
                        ..
                    }
                )
            })
            .count(),
        2
    );

    // Opting in again and updating SHOULD NOT enroll us again (we've been disqualified).
    set_global_user_participation(&db, &mut writer, true)?;

    let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &targeting_attributes);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 0);
    assert!(events.is_empty());

    assert_eq!(
        get_experiment_enrollments(&db, &writer)?
            .into_iter()
            .filter(|enr| {
                matches!(
                    enr.status,
                    EnrollmentStatus::Disqualified {
                        reason: DisqualifiedReason::OptOut,
                        ..
                    }
                )
            })
            .count(),
        2
    );

    writer.commit()?;
    Ok(())
}

#[test]
fn test_telemetry_reset() -> Result<()> {
    let _ = env_logger::try_init();
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;

    let mock_exp1_slug = "exp-1".to_string();
    let mock_exp1_branch = "branch-1".to_string();
    let mock_exp2_slug = "exp-2".to_string();
    let mock_exp2_branch = "branch-2".to_string();
    let mock_exp3_slug = "exp-3".to_string();

    // Three currently-known experiments, in different states.
    let store = db.get_store(StoreId::Enrollments);
    store.put(
        &mut writer,
        &mock_exp1_slug,
        &ExperimentEnrollment {
            slug: mock_exp1_slug.clone(),
            status: EnrollmentStatus::new_enrolled(EnrolledReason::Qualified, &mock_exp1_branch),
        },
    )?;
    store.put(
        &mut writer,
        &mock_exp2_slug,
        &ExperimentEnrollment {
            slug: mock_exp2_slug.clone(),
            status: EnrollmentStatus::Disqualified {
                reason: DisqualifiedReason::Error,
                branch: mock_exp2_branch.clone(),
                enrollment_id: Uuid::new_v4(),
            },
        },
    )?;
    store.put(
        &mut writer,
        &mock_exp3_slug,
        &ExperimentEnrollment {
            slug: mock_exp3_slug.clone(),
            status: EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted,
            },
        },
    )?;
    writer.commit()?;

    let mut writer = db.write()?;
    let events = reset_telemetry_identifiers(&db, &mut writer)?;
    writer.commit()?;

    let enrollments = db.collect_all::<ExperimentEnrollment>(StoreId::Enrollments)?;
    assert_eq!(enrollments.len(), 3);

    // The enrolled experiment should have moved to disqualified with nil enrollment_id.
    assert_eq!(enrollments[0].slug, mock_exp1_slug);
    assert!(
        matches!(&enrollments[0].status, EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::OptOut,
            branch,
            enrollment_id,
            ..
        } if *branch == mock_exp1_branch && enrollment_id.is_nil())
    );

    // The disqualified experiment should have stayed disqualified, with nil enrollment_id.
    assert_eq!(enrollments[1].slug, mock_exp2_slug);
    assert!(
        matches!(&enrollments[1].status, EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::Error,
            branch,
            enrollment_id,
            ..
        } if *branch == mock_exp2_branch && enrollment_id.is_nil())
    );

    // The not-enrolled experiment should have been unchanged.
    assert_eq!(enrollments[2].slug, mock_exp3_slug);
    assert!(matches!(
        &enrollments[2].status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted,
            ..
        }
    ));

    // We should have returned a single disqualification event.
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], EnrollmentChangeEvent {
        change: EnrollmentChangeEventType::Disqualification,
        reason: Some(reason),
        experiment_slug,
        branch_slug,
        enrollment_id,
    } if reason == "optout"
        && *experiment_slug == mock_exp1_slug
        && *branch_slug == mock_exp1_branch
        && ! Uuid::parse_str(enrollment_id)?.is_nil()
    ));

    Ok(())
}

#[test]
fn test_filter_experiments_by_closure() -> Result<()> {
    let experiment = Experiment {
        slug: "experiment1".into(),
        is_rollout: false,
        ..Default::default()
    };
    let ex_enrollment = ExperimentEnrollment {
        slug: experiment.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::EnrollmentsPaused,
        },
    };

    let rollout = Experiment {
        slug: "rollout1".into(),
        is_rollout: true,
        ..Default::default()
    };
    let ro_enrollment = ExperimentEnrollment {
        slug: rollout.slug.clone(),
        status: EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::EnrollmentsPaused,
        },
    };

    let recipes = &[experiment.clone(), rollout.clone()];
    let enrollments = &[ro_enrollment, ex_enrollment];

    let (ro, ro_enrollments) =
        filter_experiments_and_enrollments(recipes, enrollments, Experiment::is_rollout);
    assert_eq!(ro.len(), 1);
    assert_eq!(ro_enrollments.len(), 1);
    assert_eq!(ro[0].slug, rollout.slug);
    assert_eq!(ro_enrollments[0].slug, rollout.slug);

    let (experiments, exp_enrollments) =
        filter_experiments_and_enrollments(recipes, enrollments, |e| !e.is_rollout());
    assert_eq!(experiments.len(), 1);
    assert_eq!(exp_enrollments.len(), 1);
    assert_eq!(experiments[0].slug, experiment.slug);
    assert_eq!(exp_enrollments[0].slug, experiment.slug);

    Ok(())
}
