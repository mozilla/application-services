/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Testing enrollment.rs

use crate::tests::helpers::{get_bucketed_rollout, get_experiment_with_published_date};
use crate::{
    defaults::Defaults,
    enrollment::*,
    error::{debug, Result},
    tests::helpers::{
        get_multi_feature_experiment, get_single_feature_experiment, get_test_experiments,
        no_coenrolling_features,
    },
    AppContext, AvailableRandomizationUnits, Branch, BucketConfig, Experiment, FeatureConfig,
    NimbusTargetingHelper, TargetingAttributes,
};

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {
        use crate::tests::helpers::get_ios_rollout_experiment;

    }
}
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

impl From<AppContext> for NimbusTargetingHelper {
    fn from(context: AppContext) -> Self {
        cfg_if::cfg_if! {
            if #[cfg(feature = "stateful")] {
                let ta: TargetingAttributes = context.into();
            } else {
                let ta = TargetingAttributes::new(context, Default::default());
            }
        }
        ta.into()
    }
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
    let aru = AvailableRandomizationUnits::with_nimbus_id(&nimbus_id);
    (nimbus_id, app_ctx, aru)
}

fn enrollment_evolver<'a>(
    targeting_helper: &'a mut NimbusTargetingHelper,
    aru: &'a AvailableRandomizationUnits,
    ids: &'a HashSet<&str>,
) -> EnrollmentsEvolver<'a> {
    EnrollmentsEvolver::new(aru, targeting_helper, ids)
}

#[cfg(feature = "stateful")]
#[test]
fn test_ios_rollout_experiment() -> Result<()> {
    let exp = &get_ios_rollout_experiment();
    let (_, app_ctx, aru) = local_ctx();
    let app_ctx = AppContext {
        app_name: "firefox_ios".to_string(),
        app_version: Some("114.0".to_string()),
        channel: "release".to_string(),
        ..app_ctx
    };
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment::<Experiment>(true, None, Some(exp), None, &mut events)?
        .unwrap();
    println!("Enrollment: {:?}", &enrollment.status);
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled { .. }
    ));
    Ok(())
}

#[test]
fn test_evolver_new_experiment_enrolled() -> Result<()> {
    let exp = &get_test_experiments()[0];
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment::<Experiment>(true, None, Some(exp), None, &mut events)?
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment::<Experiment>(true, None, Some(&exp), None, &mut events)?
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment::<Experiment>(false, None, Some(&exp), None, &mut events)?
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let enrollment = evolver
        .evolve_enrollment::<Experiment>(true, None, Some(&exp), None, &mut events)?
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
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
        branch,
        ..
    } = enrollment.status
    {
        assert_eq!(branch, "control");
    } else {
        panic!("Wrong variant!");
    }
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_experiment_update_enrolled_then_targeting_changed() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (_, mut app_ctx, aru) = local_ctx();
    "foobar".clone_into(&mut app_ctx.app_name); // Make the experiment targeting fail.
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
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
        branch,
        ..
    } = enrollment.status
    {
        assert_eq!(branch, "control");
    } else {
        panic!("Wrong variant! \n{:#?}", enrollment.status);
    }
    assert_eq!(events.len(), 1);
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
    let exp = get_bucketed_rollout("test-rollout", 0);
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let observed = evolver
        .evolve_enrollment(
            true,
            Some(&exp),
            Some(&exp),
            Some(&existing_enrollment),
            &mut events,
        )?
        .unwrap();
    assert!(matches!(
        observed,
        ExperimentEnrollment {
            status: EnrollmentStatus::Disqualified {
                reason: DisqualifiedReason::NotSelected,
                ..
            },
            ..
        }
    ));
    assert_eq!(1, events.len());
    Ok(())
}

#[test]
fn test_rollout_unenrolls_when_bucketing_changes() -> Result<()> {
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);

    let slug = "my-rollout";

    // Start at 0%
    let ro = get_bucketed_rollout(slug, 0);
    let recipes = [ro];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(true, &[], &recipes, &[])?;

    assert_eq!(enrollments.len(), 1);
    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug);
    assert!(matches!(&enr.status, EnrollmentStatus::NotEnrolled { .. }));

    // Up to 100%
    let prev_recipes = recipes;
    let ro = get_bucketed_rollout(slug, 10_000);
    let recipes = [ro];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(
        true,
        &prev_recipes,
        &recipes,
        enrollments.as_slice(),
    )?;
    assert_eq!(enrollments.len(), 1);
    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug);
    assert!(matches!(&enr.status, EnrollmentStatus::Enrolled { .. }));

    // Back to zero again
    let prev_recipes = recipes;
    let ro = get_bucketed_rollout(slug, 0);
    let recipes = [ro];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(
        true,
        &prev_recipes,
        &recipes,
        enrollments.as_slice(),
    )?;
    assert_eq!(enrollments.len(), 1);
    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug);
    assert!(matches!(
        &enr.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::NotSelected,
            ..
        }
    ));

    Ok(())
}

#[test]
fn test_rollout_unenrolls_then_reenrolls_when_bucketing_changes() -> Result<()> {
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);

    let slug = "my-rollout";

    // Start at 0%
    let ro = get_bucketed_rollout(slug, 0);
    let recipes = [ro];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(true, &[], &recipes, &[])?;

    assert_eq!(enrollments.len(), 1);
    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug);
    assert!(matches!(&enr.status, EnrollmentStatus::NotEnrolled { .. }));

    // Up to 100%
    let prev_recipes = recipes;
    let ro = get_bucketed_rollout(slug, 10_000);
    let recipes = [ro];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(
        true,
        &prev_recipes,
        &recipes,
        enrollments.as_slice(),
    )?;
    assert_eq!(enrollments.len(), 1);
    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug);
    assert!(matches!(&enr.status, EnrollmentStatus::Enrolled { .. }));

    // Back to zero again
    let prev_recipes = recipes;
    let ro = get_bucketed_rollout(slug, 0);
    let recipes = [ro];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(
        true,
        &prev_recipes,
        &recipes,
        enrollments.as_slice(),
    )?;
    assert_eq!(enrollments.len(), 1);
    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug);
    assert!(matches!(
        &enr.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::NotSelected,
            ..
        }
    ));

    // Back up to 100%
    let prev_recipes = recipes;
    let ro = get_bucketed_rollout(slug, 10_000);
    let recipes = [ro];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(
        true,
        &prev_recipes,
        &recipes,
        enrollments.as_slice(),
    )?;
    assert_eq!(enrollments.len(), 1);
    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug);
    assert!(matches!(&enr.status, EnrollmentStatus::Enrolled { .. }));

    Ok(())
}

#[test]
fn test_experiment_does_not_reenroll_from_disqualified_not_selected_or_not_targeted() -> Result<()>
{
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);

    let slug_1 = "my-experiment-1";
    let slug_2 = "my-experiment-2";

    let exp_1 =
        get_single_feature_experiment(slug_1, "feature_1", Value::Object(Default::default()));
    let exp_2 =
        get_single_feature_experiment(slug_2, "feature_2", Value::Object(Default::default()));
    let recipes = [exp_1, exp_2];

    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(
        true,
        &recipes,
        &recipes,
        &[
            ExperimentEnrollment {
                slug: slug_1.into(),
                status: EnrollmentStatus::Disqualified {
                    reason: DisqualifiedReason::NotSelected,
                    branch: "control".into(),
                },
            },
            ExperimentEnrollment {
                slug: slug_2.into(),
                status: EnrollmentStatus::Disqualified {
                    reason: DisqualifiedReason::NotTargeted,
                    branch: "control".into(),
                },
            },
        ],
    )?;

    assert_eq!(enrollments.len(), 2);

    let enr = enrollments.first().unwrap();
    assert_eq!(&enr.slug, slug_1);
    assert!(matches!(&enr.status, EnrollmentStatus::Disqualified { .. }));

    let enr = enrollments.get(1).unwrap();
    assert_eq!(&enr.slug, slug_2);
    assert!(matches!(&enr.status, EnrollmentStatus::Disqualified { .. }));

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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
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
    exp.branches = vec![Branch {
        slug: "bobo-branch".to_owned(),
        ratio: 1,
        feature: None,
        features: None,
    }];
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
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
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    Ok(())
}

#[test]
fn test_evolver_experiment_update_disqualified_then_opted_out() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Disqualified {
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Disqualified {
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
    error_support::init_for_tests();

    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
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
    let features = map_features_by_feature_id(
        &enrollments,
        &updated_experiments,
        &no_coenrolling_features(),
    );
    assert_eq!(features.len(), 1);
    assert!(features.contains_key("about_welcome"));

    let enrolled_feature = features.get("about_welcome").unwrap();
    assert_eq!(
        Value::Object(enrolled_feature.feature.value.clone()),
        json!({ "text": "OK then", "number": 42})
    );

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
        ["newtab".to_string(), "about_welcome".to_string()]
            .iter()
            .collect::<HashSet<_>>()
    );
    Ok(())
}

#[test]
fn test_evolver_experiment_not_enrolled_feature_conflict() -> Result<()> {
    error_support::init_for_tests();

    let mut test_experiments = get_test_experiments();
    test_experiments.push(get_conflicting_experiment());
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);
    let (enrollments, events) =
        evolver.evolve_enrollments::<Experiment>(true, &[], &test_experiments, &[])?;

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

    debug!("events: {:?}", events);

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
    let (_, app_ctx, aru) = local_ctx();
    let mut targeting_attributes = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_attributes, &ids);
    let (enrollments, events) =
        evolver.evolve_enrollments::<Experiment>(true, &[], &test_experiments, &[])?;

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
    error_support::init_for_tests();

    let test_experiments = get_test_experiments();
    let (_, app_ctx, aru) = local_ctx();
    let mut targeting_attributes = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_attributes, &ids);
    let (enrollments, _) =
        evolver.evolve_enrollments::<Experiment>(true, &[], &test_experiments, &[])?;

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

    debug!("events = {:?}", events);

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
    error_support::init_for_tests();

    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);

    let aboutwelcome_experiment = get_experiment_with_aboutwelcome_feature_branches();
    let newtab_experiment = get_experiment_with_newtab_feature_branches();
    let mixed_experiment = get_experiment_with_different_feature_branches();
    let multi_feature_experiment = get_experiment_with_different_features_same_branch();
    // 1. we have two experiments that use one feature each. There's no conflicts.
    let next_experiments = vec![aboutwelcome_experiment.clone(), newtab_experiment.clone()];

    let (enrollments, _) =
        evolver.evolve_enrollments::<Experiment>(true, &[], &next_experiments, &[])?;

    let feature_map =
        map_features_by_feature_id(&enrollments, &next_experiments, &no_coenrolling_features());
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
        [
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

    let feature_map =
        map_features_by_feature_id(&enrollments, &next_experiments, &no_coenrolling_features());
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
        [
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

    let feature_map =
        map_features_by_feature_id(&enrollments, &next_experiments, &no_coenrolling_features());
    assert_eq!(feature_map.len(), 1);
    assert!(!feature_map.contains_key("about_welcome"));
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
        ["newtab-feature-experiment"]
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

    let feature_map =
        map_features_by_feature_id(&enrollments, &next_experiments, &no_coenrolling_features());
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
        ["mixed-feature-experiment"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
    );

    // 4. Starting from an empty enrollments, enroll a multi-feature and then add the single feature ones back in again, which won't be able to enroll.
    // 4a. The multi feature experiment.
    let prev_enrollments = vec![];
    let prev_experiments = vec![];
    let next_experiments = vec![mixed_experiment.clone()];
    let (enrollments, _) = evolver.evolve_enrollments::<Experiment>(
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
    let feature_map =
        map_features_by_feature_id(&enrollments, &next_experiments, &no_coenrolling_features());
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
        ["mixed-feature-experiment"]
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
    let feature_map =
        map_features_by_feature_id(&enrollments, &next_experiments, &no_coenrolling_features());
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
        ["mixed-feature-experiment"]
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

    let feature_map =
        map_features_by_feature_id(&enrollments, &next_experiments, &no_coenrolling_features());
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
        ["multi-feature-experiment"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
    );

    Ok(())
}

#[test]
fn test_evolver_experiment_update_was_enrolled() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::WasEnrolled {
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
fn test_map_features_by_feature_id_with_coenrollment() -> Result<()> {
    let exp1 = get_single_feature_experiment("exp1", "colliding", json!({"x": 1 }));
    let exp2 = get_single_feature_experiment("exp2", "coenrolling", json!({ "a": 1, "b": 2 }));
    let exp3 = get_single_feature_experiment("exp3", "coenrolling", json!({ "b": 3, "c": 4 }));

    let ids = HashSet::from(["coenrolling"]);
    let exps = [exp1, exp2, exp3];

    let observed = map_features_by_feature_id(&[], &exps, &ids);
    let expected = Default::default();
    assert_eq!(observed, expected);

    let enr1 = ExperimentEnrollment::not_enrolled("exp1");
    let enr2 = ExperimentEnrollment::not_enrolled("exp2");
    let enr3 = ExperimentEnrollment::not_enrolled("exp3");
    let enrollments = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrollments, &exps, &ids);
    let expected = Default::default();
    assert_eq!(observed, expected);

    let enr1 = ExperimentEnrollment::enrolled("exp1");
    let enr2 = ExperimentEnrollment::not_enrolled("exp2");
    let enr3 = ExperimentEnrollment::not_enrolled("exp3");
    let enrollments = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrollments, &exps, &ids);
    let expected = HashMap::from([(
        "colliding".to_string(),
        EnrolledFeatureConfig::new("colliding", json!({"x": 1 }), "exp1", Some("control")),
    )]);
    assert_eq!(observed, expected);

    let enr1 = ExperimentEnrollment::enrolled("exp1");
    let enr2 = ExperimentEnrollment::enrolled("exp2");
    let enr3 = ExperimentEnrollment::not_enrolled("exp3");
    let enrollments = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrollments, &exps, &ids);
    let expected = HashMap::from([
        (
            "colliding".to_string(),
            EnrolledFeatureConfig::new("colliding", json!({"x": 1 }), "exp1", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({"a": 1, "b": 2, }),
                "exp2",
                Some("control"),
            ),
        ),
    ]);
    assert_eq!(observed, expected);

    let enr1 = ExperimentEnrollment::enrolled("exp1");
    let enr2 = ExperimentEnrollment::enrolled("exp2");
    let enr3 = ExperimentEnrollment::enrolled("exp3");
    let enrollments = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrollments, &exps, &ids);
    let expected = HashMap::from([
        (
            "colliding".to_string(),
            EnrolledFeatureConfig::new("colliding", json!({"x": 1 }), "exp1", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "a": 1, // from exp2
                    "b": 3, // from exp3
                    "c": 4, // from exp3
                }),
                "exp2+exp3",
                None,
            ),
        ),
    ]);
    assert_eq!(observed, expected);
    Ok(())
}

#[test]
fn test_map_features_by_feature_id_with_coenrolling_multifeature() -> Result<()> {
    let exp1 = get_multi_feature_experiment(
        "exp1",
        vec![
            ("colliding1", json!({"x": 1 })),
            ("coenrolling", json!({ "a": 1, "b": 2 })),
        ],
    );
    let exp2 = get_single_feature_experiment("exp2", "coenrolling", json!({ "b": 3, "c": 4 }));
    let exp3 = get_multi_feature_experiment(
        "exp3",
        vec![
            ("colliding2", json!({"y": 1 })),
            ("coenrolling", json!({ "c": 5, "d": 6 })),
        ],
    );

    let ids = HashSet::from(["coenrolling"]);
    let exps = [exp1, exp2, exp3];

    let enr1 = ExperimentEnrollment::enrolled("exp1");
    let enr2 = ExperimentEnrollment::not_enrolled("exp2");
    let enr3 = ExperimentEnrollment::not_enrolled("exp3");
    let enrollments = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrollments, &exps, &ids);
    let expected = HashMap::from([
        (
            "colliding1".to_string(),
            EnrolledFeatureConfig::new("colliding1", json!({"x": 1 }), "exp1", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({"a": 1, "b": 2, }),
                "exp1",
                Some("control"),
            ),
        ),
    ]);
    assert_eq!(observed, expected);

    let enr1 = ExperimentEnrollment::enrolled("exp1");
    let enr2 = ExperimentEnrollment::enrolled("exp2");
    let enr3 = ExperimentEnrollment::not_enrolled("exp3");
    let enrollments = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrollments, &exps, &ids);
    let expected = HashMap::from([
        (
            "colliding1".to_string(),
            EnrolledFeatureConfig::new("colliding1", json!({"x": 1 }), "exp1", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "a": 1, // from exp1
                    "b": 3, // from exp2
                    "c": 4, // from exp2
                }),
                "exp1+exp2",
                None,
            ),
        ),
    ]);
    assert_eq!(observed, expected);

    let enr1 = ExperimentEnrollment::enrolled("exp1");
    let enr2 = ExperimentEnrollment::enrolled("exp2");
    let enr3 = ExperimentEnrollment::enrolled("exp3");
    let enrollments = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrollments, &exps, &ids);
    let expected = HashMap::from([
        (
            "colliding1".to_string(),
            EnrolledFeatureConfig::new("colliding1", json!({"x": 1 }), "exp1", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "a": 1, // from exp1
                    "b": 3, // from exp2
                    "c": 5, // from exp3
                    "d": 6, // from exp3
                }),
                "exp1+exp2+exp3",
                None,
            ),
        ),
        (
            "colliding2".to_string(),
            EnrolledFeatureConfig::new("colliding2", json!({"y": 1 }), "exp3", Some("control")),
        ),
    ]);
    assert_eq!(observed, expected);

    Ok(())
}

#[test]
fn test_evolve_enrollments_with_coenrolling_features() -> Result<()> {
    error_support::init_for_tests();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = HashSet::from(["coenrolling"]);
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);

    let exp1 = get_single_feature_experiment("exp1", "colliding", json!({"x": 1 }));
    let exp2 = get_single_feature_experiment("exp2", "coenrolling", json!({ "a": 1, "b": 2 }));
    let exp3 = get_single_feature_experiment("exp3", "coenrolling", json!({ "b": 3, "c": 4 }));
    let exp4 = get_single_feature_experiment("exp4", "colliding", json!({"x": 2 }));

    let all_experiments = [exp1, exp2, exp3.clone(), exp4.clone()];
    let no_experiments: [Experiment; 0] = [];

    let (enrollments, _) =
        evolver.evolve_enrollment_recipes(true, &no_experiments, &all_experiments, &[])?;

    let observed = map_features_by_feature_id(&enrollments, &all_experiments, &ids);
    let expected = HashMap::from([
        (
            "colliding".to_string(),
            EnrolledFeatureConfig::new("colliding", json!({"x": 1 }), "exp1", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "a": 1, // from exp2
                    "b": 3, // from exp3
                    "c": 4, // from exp3
                }),
                "exp2+exp3",
                None,
            ),
        ),
    ]);
    assert_eq!(observed, expected);

    let experiments = [exp3, exp4];
    let (enrollments, _) =
        evolver.evolve_enrollment_recipes(true, &all_experiments, &experiments, &enrollments)?;

    let observed = map_features_by_feature_id(&enrollments, &all_experiments, &ids);
    let expected = HashMap::from([
        (
            "colliding".to_string(),
            EnrolledFeatureConfig::new("colliding", json!({"x": 2 }), "exp4", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "b": 3, // from exp3
                    "c": 4, // from exp3
                }),
                "exp3",
                Some("control"),
            ),
        ),
    ]);
    assert_eq!(observed, expected);
    Ok(())
}

#[test]
fn test_evolve_enrollments_with_coenrolling_multi_features() -> Result<()> {
    error_support::init_for_tests();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = HashSet::from(["coenrolling"]);
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);

    let exp1 = get_multi_feature_experiment(
        "exp1",
        vec![
            ("colliding", json!({"x": 1 })),
            ("coenrolling", json!({ "a": 1, "b": 2 })),
        ],
    );
    let exp2 = get_single_feature_experiment("exp2", "coenrolling", json!({ "b": 3, "c": 4 }));
    let exp3 = get_multi_feature_experiment(
        "exp3",
        vec![
            ("colliding", json!({"y": 1 })),
            ("coenrolling", json!({ "c": 5, "d": 6 })),
        ],
    );
    let exp4 = get_multi_feature_experiment(
        "exp4",
        vec![
            ("another", json!({"p": 1 })),
            ("coenrolling", json!({ "d": 7, "e": 8 })),
        ],
    );

    let all_experiments = [exp1, exp2, exp3.clone(), exp4.clone()];
    let no_experiments: [Experiment; 0] = [];

    let (enrollments, _) =
        evolver.evolve_enrollment_recipes(true, &no_experiments, &all_experiments, &[])?;

    let observed = map_features_by_feature_id(&enrollments, &all_experiments, &ids);
    let expected = HashMap::from([
        (
            "colliding".to_string(),
            EnrolledFeatureConfig::new("colliding", json!({"x": 1 }), "exp1", Some("control")),
        ),
        (
            "another".to_string(),
            EnrolledFeatureConfig::new("another", json!({"p": 1 }), "exp4", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "a": 1, // from exp1
                    "b": 3, // from exp2
                    "c": 4, // from exp2
                    "d": 7, // from exp4
                    "e": 8, // from exp4
                }),
                "exp1+exp2+exp4",
                None,
            ),
        ),
    ]);
    assert_eq!(observed, expected);

    let experiments = [exp3, exp4];
    let (enrollments, _) =
        evolver.evolve_enrollment_recipes(true, &all_experiments, &experiments, &enrollments)?;

    let observed = map_features_by_feature_id(&enrollments, &all_experiments, &ids);
    let expected = HashMap::from([
        (
            "colliding".to_string(),
            EnrolledFeatureConfig::new("colliding", json!({"y": 1 }), "exp3", Some("control")),
        ),
        (
            "another".to_string(),
            EnrolledFeatureConfig::new("another", json!({"p": 1 }), "exp4", Some("control")),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "c": 5, // from exp3
                    "d": 6, // from exp4
                    "e": 8, // from exp4
                }),
                // This appears strange and non-deterministic, but is not:
                // the existing enrollments (i.e. for 'exp4') are processed first, then the ones that
                // are not yet enrolled (i.e. 'exp3').
                "exp4+exp3",
                None,
            ),
        ),
    ]);
    assert_eq!(observed, expected);

    Ok(())
}

#[test]
fn test_map_features_by_feature_id_with_slug_replacement() -> Result<()> {
    let exp1 =
        get_single_feature_experiment("exp1", "colliding", json!({ "experiment": "{experiment}" }));
    let exp2 = get_single_feature_experiment(
        "exp2",
        "coenrolling",
        json!(
            {
                "merging": {
                    "m2": {
                        // 👀The user types "{experiment}"…
                        "slug": "{experiment}",
                    }
                }
            }
        ),
    );
    let exp3 = get_single_feature_experiment(
        "exp3",
        "coenrolling",
        json!(
            {
                "merging": {
                    "m3": {
                        "slug": "{experiment}",
                    }
                }
            }
        ),
    );

    let ids = HashSet::from(["coenrolling"]);
    let exps = [exp1, exp2, exp3];

    let enr1 = ExperimentEnrollment::enrolled("exp1");
    let enr2 = ExperimentEnrollment::enrolled("exp2");
    let enr3 = ExperimentEnrollment::enrolled("exp3");
    let enrs = [enr1, enr2, enr3];

    let observed = map_features_by_feature_id(&enrs, &exps, &ids);
    let expected = HashMap::from([
        (
            "colliding".to_string(),
            EnrolledFeatureConfig::new(
                "colliding",
                json!({"experiment": "exp1"}),
                "exp1",
                Some("control"),
            ),
        ),
        (
            "coenrolling".to_string(),
            EnrolledFeatureConfig::new(
                "coenrolling",
                json!({
                    "merging": {
                        "m2": {
                            // 👀… and it gets replaced by the actual experiment slug,
                            "slug": "exp2",
                        },
                        "m3": {
                            // 👀… so that the different parts of the configuration coming from
                            // different experiments can identify those experiments.
                            "slug": "exp3",
                        }
                    }
                }),
                "exp2+exp3",
                None,
            ),
        ),
    ]);
    assert_eq!(observed, expected);

    Ok(())
}

#[test]
fn test_evolve_enrollments_error_handling() -> Result<()> {
    let existing_enrollments = [ExperimentEnrollment {
        slug: "secure-gold".to_owned(),
        status: EnrollmentStatus::Enrolled {
            branch: "hello".to_owned(), // XXX this OK?
            reason: EnrolledReason::Qualified,
        },
    }];

    error_support::init_for_tests();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);

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
    let (enrollments, events) = evolver.evolve_enrollments::<Experiment>(
        true,
        &[],
        &test_experiments,
        &existing_enrollments[..],
    )?;

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
    error_support::init_for_tests();
    let (_, mut app_ctx, aru) = local_ctx();
    let mut th = app_ctx.clone().into();
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);

    // The targeting for this experiment is
    // "app_id == 'org.mozilla.fenix' || is_already_enrolled"
    let test_experiment = get_is_already_enrolled_targeting_experiment();
    let test_experiments = &[test_experiment];
    // The user should get enrolled, since the targeting is OR'ing the app_id == 'org.mozilla.fenix'
    // and the 'is_already_enrolled'
    let (enrollments, events) =
        evolver.evolve_enrollments::<Experiment>(true, &[], test_experiments, &[])?;
    assert_eq!(
        enrollments.len(),
        1,
        "One enrollment should have been returned"
    );

    assert_eq!(events.len(), 1, "One event should have been returned");

    // we change the app_id so the targeting will only target
    // against the `is_already_enrolled`
    app_ctx.app_id = "org.mozilla.bobo".into();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);

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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Enrolled {
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
    if let EnrollmentStatus::WasEnrolled { branch, .. } = enrollment.status {
        assert_eq!(branch, "control");
    } else {
        panic!("Wrong variant!");
    }
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    assert_eq!(events[0].change, EnrollmentChangeEventType::Unenrollment);
    Ok(())
}

#[test]
fn test_evolver_experiment_ended_was_disqualified() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: EnrollmentStatus::Disqualified {
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
    if let EnrollmentStatus::WasEnrolled { branch, .. } = enrollment.status {
        assert_eq!(branch, "control");
    } else {
        panic!("Wrong variant!");
    }
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].experiment_slug, exp.slug);
    assert_eq!(events[0].branch_slug, "control");
    assert_eq!(events[0].change, EnrollmentChangeEventType::Unenrollment);
    Ok(())
}

#[test]
fn test_evolver_experiment_ended_was_not_enrolled() -> Result<()> {
    let exp = get_test_experiments()[0].clone();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: "secure-gold".to_owned(),
        status: EnrollmentStatus::WasEnrolled {
            branch: "control".to_owned(),
            experiment_ended_at: now_secs(),
        },
    };
    let enrollment = evolver.evolve_enrollment::<Experiment>(
        true,
        None,
        None,
        Some(&existing_enrollment),
        &mut events,
    )?;
    assert_eq!(enrollment.unwrap(), existing_enrollment);
    assert!(events.is_empty());
    Ok(())
}

#[test]
fn test_evolver_garbage_collection_after_threshold() -> Result<()> {
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let mut events = vec![];
    let existing_enrollment = ExperimentEnrollment {
        slug: "secure-gold".to_owned(),
        status: EnrollmentStatus::WasEnrolled {
            branch: "control".to_owned(),
            experiment_ended_at: now_secs() - PREVIOUS_ENROLLMENTS_GC_TIME.as_secs() - 60,
        },
    };
    let enrollment = evolver.evolve_enrollment::<Experiment>(
        true,
        None,
        None,
        Some(&existing_enrollment),
        &mut events,
    )?;
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
            branch: "control".to_owned(),
            experiment_ended_at: now_secs(),
        },
    };
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let res = evolver.evolve_enrollment::<Experiment>(
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
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let res = evolver.evolve_enrollment(true, Some(&exp), Some(&exp), None, &mut vec![]);
    assert!(res.is_err());
}

#[test]
#[should_panic]
fn test_evolver_no_experiments_no_enrollment() {
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    evolver
        .evolve_enrollment::<Experiment>(true, None, None, None, &mut vec![])
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

    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let (enrollments, events) =
        evolver.evolve_enrollments::<Experiment>(true, &[], recipes, &[])?;
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

    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);
    let (enrollments, events) =
        evolver.evolve_enrollments::<Experiment>(true, &[], recipes, &[])?;
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
            reason: EnrolledReason::Qualified,
        },
    };

    let ro_enrollment = ExperimentEnrollment {
        slug: ro_slug.clone(),
        status: EnrollmentStatus::Enrolled {
            branch: ro_slug,
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollments = &[ro_enrollment, exp_enrollment];
    let experiments = &[experiment, rollout];
    let features = map_features_by_feature_id(enrollments, experiments, &no_coenrolling_features());

    assert_alice_bob_charlie(&features);
    Ok(())
}

#[test]
fn test_rollouts_end_to_end() -> Result<()> {
    let (rollout, experiment) = get_rollout_and_experiment();
    let recipes = &[rollout, experiment];

    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = no_coenrolling_features();
    let mut evolver = enrollment_evolver(&mut th, &aru, &ids);

    let (enrollments, _events) =
        evolver.evolve_enrollments::<Experiment>(true, &[], recipes, &[])?;

    let features = map_features_by_feature_id(&enrollments, recipes, &no_coenrolling_features());

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
    let existing_enrollment = ExperimentEnrollment {
        slug: exp.slug,
        status: EnrollmentStatus::Enrolled {
            branch: "control".to_owned(),
            reason: EnrolledReason::Qualified,
        },
    };
    let enrollment = existing_enrollment.on_explicit_opt_out(&mut events);
    if let EnrollmentStatus::Disqualified { branch, .. } = enrollment.status {
        assert_eq!(branch, "control");
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
            branch: "control".to_owned(),
            reason: DisqualifiedReason::NotTargeted,
        },
    };
    let enrollment = existing_enrollment.on_explicit_opt_out(&mut events);
    assert_eq!(enrollment, existing_enrollment);
    assert!(events.is_empty());
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

#[test]
fn test_populate_feature_maps() -> Result<()> {
    let coenrolling_ids = HashSet::from(["coenrolling"]);
    let mut colliding_map = Default::default();
    let mut coenrolling_map = Default::default();

    populate_feature_maps(
        EnrolledFeatureConfig::new("colliding", json!({}), "exp1", None),
        &coenrolling_ids,
        &mut colliding_map,
        &mut coenrolling_map,
    );

    assert!(colliding_map.contains_key("colliding"));
    assert!(!coenrolling_map.contains_key("colliding"));

    // Add a config for 'coenrolling' feature
    let added = EnrolledFeatureConfig::new(
        "coenrolling",
        json!({
            "a": 1,
            "b": 2,
        }),
        "exp2",
        None,
    );
    populate_feature_maps(
        added.clone(),
        &coenrolling_ids,
        &mut colliding_map,
        &mut coenrolling_map,
    );

    let expected = added;

    assert!(!colliding_map.contains_key("coenrolling"));
    assert!(coenrolling_map.contains_key("coenrolling"));

    let observed = coenrolling_map.get("coenrolling");
    assert!(observed.is_some());

    let observed = observed.unwrap();
    assert_eq!(&expected, observed);

    // Add a second config for the 'coenrolling' feature.
    let added = EnrolledFeatureConfig::new(
        "coenrolling",
        json!({
            "b": 3,
            "c": 4,
        }),
        "exp3",
        None,
    );

    populate_feature_maps(
        added,
        &coenrolling_ids,
        &mut colliding_map,
        &mut coenrolling_map,
    );

    let expected = EnrolledFeatureConfig::new(
        "coenrolling",
        json!({
            "a": 1, // from 'exp2'
            "b": 3, // from 'exp3'
            "c": 4, // from 'exp3'
        }),
        "exp2+exp3",
        None,
    );

    assert!(!colliding_map.contains_key("coenrolling"));
    assert!(coenrolling_map.contains_key("coenrolling"));

    let observed = coenrolling_map.get("coenrolling");
    assert!(observed.is_some());

    let observed = observed.unwrap();
    assert_eq!(&expected, observed);

    Ok(())
}

#[test]
fn test_sort_experiments_by_published_date() -> Result<()> {
    let slug_1 = "slug-1";
    let slug_2 = "slug-2";
    let slug_3 = "slug-3";
    let slug_4 = "slug-4";
    let experiments = vec![
        get_experiment_with_published_date(slug_1, None),
        get_experiment_with_published_date(slug_2, Some("2023-11-21T18:00:00Z".into())),
        get_experiment_with_published_date(slug_3, None),
        get_experiment_with_published_date(slug_4, Some("2023-11-21T15:00:00Z".into())),
    ];
    let result = sort_experiments_by_published_date(&experiments);

    assert_eq!(slug_1, result[0].slug);
    assert_eq!(slug_3, result[1].slug);
    assert_eq!(slug_4, result[2].slug);
    assert_eq!(slug_2, result[3].slug);
    Ok(())
}

#[test]
fn test_evolve_enrollments_ordering() -> Result<()> {
    error_support::init_for_tests();
    let (_, app_ctx, aru) = local_ctx();
    let mut th = app_ctx.into();
    let ids = HashSet::new();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);

    let exp1 = get_single_feature_experiment("slug-1", "colliding-feature", json!({"x": 1 }))
        .patch(json!({"publishedDate": "2023-11-21T18:00:00Z"}));
    let exp2 = get_single_feature_experiment("slug-2", "colliding-feature", json!({"x": 2 }))
        .patch(json!({"publishedDate": "2023-11-21T15:00:00Z"}));

    let all_experiments = [exp1, exp2];
    let no_experiments: [Experiment; 0] = [];

    let (enrollments, _) =
        evolver.evolve_enrollment_recipes(true, &no_experiments, &all_experiments, &[])?;

    let observed = map_features_by_feature_id(&enrollments, &all_experiments, &ids);
    let expected = HashMap::from([(
        "colliding-feature".to_string(),
        EnrolledFeatureConfig::new(
            "colliding-feature",
            json!({"x": 2 }),
            "slug-2",
            Some("control"),
        ),
    )]);
    assert_eq!(observed, expected);

    Ok(())
}
