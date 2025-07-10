/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Older tests that also use the DB.
// XXX: make them less complicated (since the transitions are covered in crate::tests::test_enrollment), just see if we write to the DB properly.

use crate::{
    enrollment::{
        DisqualifiedReason, EnrolledReason, EnrollmentChangeEvent, EnrollmentChangeEventType,
        EnrollmentStatus, EnrollmentsEvolver, ExperimentEnrollment, NotEnrolledReason,
    },
    stateful::{
        behavior::EventStore,
        enrollment::{
            get_enrollments, opt_in_with_branch, opt_out, reset_telemetry_identifiers,
            set_global_user_participation,
        },
        persistence::{Database, Readable, StoreId},
    },
    tests::helpers::{get_test_experiments, no_coenrolling_features},
    AppContext, AvailableRandomizationUnits, NimbusTargetingHelper, Result,
};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn get_experiment_enrollments<'r>(
    db: &Database,
    reader: &'r impl Readable<'r>,
) -> Result<Vec<ExperimentEnrollment>> {
    db.get_store(StoreId::Enrollments).collect_all(reader)
}

impl From<EventStore> for NimbusTargetingHelper {
    fn from(value: EventStore) -> Self {
        let ctx: AppContext = Default::default();
        NimbusTargetingHelper::new(ctx, Arc::new(Mutex::new(value)), None)
    }
}

#[test]
fn test_enrollments() -> Result<()> {
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;
    let exp1 = get_test_experiments()[0].clone();
    let nimbus_id = Uuid::new_v4();
    let aru = AvailableRandomizationUnits::with_nimbus_id(&nimbus_id);
    let mut targeting_attributes = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();
    assert_eq!(get_enrollments(&db, &writer)?.len(), 0);

    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_attributes, &ids);
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
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;
    let nimbus_id = Uuid::new_v4();
    let aru = AvailableRandomizationUnits::with_nimbus_id(&nimbus_id);
    let mut th = NimbusTargetingHelper::from(AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    });
    assert_eq!(get_enrollments(&db, &writer)?.len(), 0);
    let exps = get_test_experiments();

    let ids = no_coenrolling_features();
    let mut targeting_helper = th.clone();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_helper, &ids);
    let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

    let enrollments = get_enrollments(&db, &writer)?;
    assert_eq!(enrollments.len(), 2);
    assert_eq!(events.len(), 2);

    // pretend we just updated from the server and one of the 2 is missing.
    let exps = &[exps[1].clone()];
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);
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
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;
    let nimbus_id = Uuid::new_v4();
    let mut th = NimbusTargetingHelper::from(AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    });
    let aru = AvailableRandomizationUnits::with_nimbus_id(&nimbus_id);
    assert_eq!(get_enrollments(&db, &writer)?.len(), 0);
    let exps = get_test_experiments();

    // User has opted out of new experiments.
    set_global_user_participation(&db, &mut writer, false)?;

    let ids = no_coenrolling_features();
    let mut targeting_helper = th.clone();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_helper, &ids);
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

    let mut targeting_helper = th.clone();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_helper, &ids);
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

    let mut targeting_helper = th.clone();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_helper, &ids);
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

    let mut evolver = EnrollmentsEvolver::new(&aru, &mut th, &ids);
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
    error_support::init_for_tests();
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

    // The enrolled experiment should have moved to disqualified.
    assert_eq!(enrollments[0].slug, mock_exp1_slug);
    assert!(
        matches!(&enrollments[0].status, EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::OptOut,
            branch,
            ..
        } if *branch == mock_exp1_branch)
    );

    // The disqualified experiment should have stayed disqualified.
    assert_eq!(enrollments[1].slug, mock_exp2_slug);
    assert!(
        matches!(&enrollments[1].status, EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::Error,
            branch,
            ..
        } if *branch == mock_exp2_branch)
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
    } if reason == "optout"
        && *experiment_slug == mock_exp1_slug
        && *branch_slug == mock_exp1_branch
    ));

    Ok(())
}
