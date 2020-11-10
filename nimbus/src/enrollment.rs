// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
use crate::error::Result;
use crate::evaluator::evaluate_enrollment;
use crate::persistence::{Database, StoreId, Writer};
use crate::{AppContext, AvailableRandomizationUnits, EnrolledExperiment, Experiment};

use ::uuid::Uuid;
use serde_derive::*;
use std::collections::{HashMap, HashSet};

const DB_KEY_GLOBAL_USER_PARTICIPATION: &str = "user-opt-in";
const DEFAULT_GLOBAL_USER_PARTICIPATION: bool = true;

// These are types we use internally for managing enrollments.
#[derive(Deserialize, Serialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum EnrolledReason {
    Qualified, // A normal enrollment as per the experiment's rules.
    OptIn,     // Explicit opt-in.
}

// Every experiment has an ExperimentEnrollment, even when we aren't enrolled.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct ExperimentEnrollment {
    pub slug: String,
    pub status: EnrollmentStatus,
}

#[derive(Deserialize, Serialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum EnrollmentStatus {
    // Enrolled
    Enrolled {
        reason: EnrolledReason,
        branch: String,
    },
    // Not enrolled because our evaluator declined to choose us.
    NotSelected,
    // Not enrolled because either enrollment is paused or the experiment is over.
    NotRunning,
    // Not enrolled because we are not being targeted for this experiment.
    NotTargeted,
    // User explicitly opted out.
    OptedOut,
    // There was some error opting in.
    Error {
        // Ideally this would be an Error, but then we'd need to make Error
        // serde compatible, which isn't trivial nor desirable.
        reason: String,
    },
}

impl EnrollmentStatus {
    // This is used in examples, but not in the main dylib.
    #[allow(dead_code)]
    pub fn is_enrolled(&self) -> bool {
        matches!(self, EnrollmentStatus::Enrolled { .. })
    }
}

/// Return information about all enrolled experiments.
pub fn get_enrollments(db: &Database) -> Result<Vec<EnrolledExperiment>> {
    let enrollments: Vec<ExperimentEnrollment> = db.collect_all(StoreId::Enrollments)?;
    let mut result = Vec::with_capacity(enrollments.len());
    for enrollment in enrollments {
        log::debug!("Have enrollment: {:?}", enrollment);
        if let EnrollmentStatus::Enrolled { ref branch, .. } = enrollment.status {
            if let Some(experiment) =
                db.get::<Experiment>(StoreId::Experiments, &enrollment.slug)?
            {
                result.push(EnrolledExperiment {
                    slug: experiment.slug,
                    user_facing_name: experiment.user_facing_name,
                    user_facing_description: experiment.user_facing_description,
                    branch_slug: branch.to_string(),
                });
            } else {
                log::warn!(
                    "Have enrollment {:?} but no matching experiment!",
                    enrollment
                );
            }
        }
    }
    Ok(result)
}

/// Update all enrollments. Typically used immediately after the list of
/// experiments has been refreshed, so some might mean new enrollments, some
/// might have expired, etc.
pub fn update_enrollments(
    db: &Database,
    mut writer: &mut Writer,
    nimbus_id: &Uuid,
    aru: &AvailableRandomizationUnits,
    app_context: &AppContext,
) -> Result<()> {
    log::info!("updating enrollments...");
    // We might have enrollments for experiments which no longer exist, so we
    // first build a set of all IDs in both groups.
    let mut all_slugs = HashSet::new();
    let experiments = db
        .get_store(StoreId::Experiments)
        .collect_all::<Experiment>(writer)?;
    let mut map_experiments = HashMap::with_capacity(experiments.len());
    for e in experiments {
        all_slugs.insert(e.slug.clone());
        map_experiments.insert(e.slug.clone(), e);
    }
    // and existing enrollments.
    let store = db.get_store(StoreId::Enrollments);
    let enrollments = store.collect_all::<ExperimentEnrollment>(&writer)?;
    let mut map_enrollments = HashMap::with_capacity(enrollments.len());
    for e in enrollments {
        all_slugs.insert(e.slug.clone());
        map_enrollments.insert(e.slug.clone(), e);
    }

    // The user may have opted out from experiments altogether.
    let is_user_participating = get_global_user_participation(db, writer)?;

    // XXX - we want to emit events for many of these things, but we are hoping
    // we can put that off until we have a glean rust sdk available and thus
    // avoid the complexity of passing the info to glean via kotlin/swift/js.
    for slug in all_slugs.iter() {
        match (map_experiments.get(slug), map_enrollments.get(slug)) {
            (Some(_), Some(enr)) => {
                // XXX - should check:
                // * is enrollment was previously paused it may not be now.
                // * is it still active?
                // * the branch still exists, etc?
                if !is_user_participating && enr.status.is_enrolled() {
                    let enr = ExperimentEnrollment {
                        slug: slug.clone(),
                        status: EnrollmentStatus::OptedOut,
                    };
                    log::debug!(
                        "Experiment '{}' already has updated enrollment {:?}",
                        slug,
                        enr
                    );
                    store.put(&mut writer, slug, &enr)?;
                } else if is_user_participating && enr.status == EnrollmentStatus::OptedOut {
                    reset_enrollment(db, writer, slug, nimbus_id, aru, app_context)?;
                } else {
                    log::debug!("Experiment '{}' already has enrollment {:?}", slug, enr)
                }
            }
            (Some(exp), None) => {
                if is_user_participating {
                    let enr = evaluate_enrollment(nimbus_id, aru, app_context, &exp)?;
                    log::debug!(
                        "Experiment '{}' is new - enrollment status is {:?}",
                        slug,
                        enr
                    );
                    store.put(&mut writer, slug, &enr)?;
                }
            }
            (None, Some(enr)) => {
                log::debug!(
                    "Experiment '{}' vanished while we had enrollment status of {:?}",
                    slug,
                    enr
                );
                store.delete(&mut writer, slug)?;
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

/// Resets an experiment to remove any opt-in or opt-out overrides.
pub fn reset_enrollment(
    db: &Database,
    writer: &mut Writer,
    experiment_slug: &str,
    nimbus_id: &Uuid,
    aru: &AvailableRandomizationUnits,
    app_context: &AppContext,
) -> Result<()> {
    let exp_store = db.get_store(StoreId::Experiments);
    let exp = match exp_store.get::<Experiment>(&writer, experiment_slug)? {
        None => {
            // XXX - do we want specific errors for this kind of thing?
            log::warn!("No such experiment '{}'", experiment_slug);
            return Ok(());
        }
        Some(e) => e,
    };
    let enrollment = evaluate_enrollment(nimbus_id, aru, app_context, &exp)?;
    log::debug!(
        "Experiment '{}' reset enrollment {:?}",
        experiment_slug,
        enrollment
    );
    let enr_store = db.get_store(StoreId::Enrollments);
    enr_store.put(writer, experiment_slug, &enrollment)?;
    Ok(())
}

pub fn opt_in_with_branch(db: &Database, experiment_slug: &str, branch: &str) -> Result<()> {
    // For now we don't bother checking if the experiment or branch exist - if
    // they don't the enrollment will just be removed next time we refresh.
    let enrollment = ExperimentEnrollment {
        slug: experiment_slug.to_string(),
        status: EnrollmentStatus::Enrolled {
            reason: EnrolledReason::OptIn,
            branch: branch.to_string(),
        },
    };
    let mut writer = db.write()?;
    db.get_store(StoreId::Enrollments)
        .put(&mut writer, experiment_slug, &enrollment)?;
    writer.commit()?;
    Ok(())
}

pub fn opt_out(db: &Database, experiment_slug: &str) -> Result<()> {
    // As above - check experiment exists?
    let enrollment = ExperimentEnrollment {
        slug: experiment_slug.to_string(),
        status: EnrollmentStatus::OptedOut,
    };
    let mut writer = db.write()?;
    db.get_store(StoreId::Enrollments)
        .put(&mut writer, experiment_slug, &enrollment)?;
    writer.commit()?;
    Ok(())
}

pub fn get_global_user_participation(db: &Database, writer: &Writer) -> Result<bool> {
    let store = db.get_store(StoreId::Meta);
    let opted_in = store.get(writer, DB_KEY_GLOBAL_USER_PARTICIPATION)?;
    if let Some(opted_in) = opted_in {
        Ok(opted_in)
    } else {
        Ok(DEFAULT_GLOBAL_USER_PARTICIPATION)
    }
}

pub fn set_global_user_participation(
    db: &Database,
    writer: &mut Writer,
    opt_in: bool,
) -> Result<()> {
    let store = db.get_store(StoreId::Meta);
    store.put(writer, DB_KEY_GLOBAL_USER_PARTICIPATION, &opt_in)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{Database, StoreId};
    use serde_json::json;
    use tempdir::TempDir;

    fn get_test_experiments() -> Vec<serde_json::Value> {
        vec![
            json!({
                "schemaVersion": "1.0.0",
                "slug": "secure-gold",
                "endDate": null,
                "branches":[
                    {"slug": "control", "ratio": 1},
                    {"slug": "treatment","ratio":1}
                ],
                "probeSets":[],
                "startDate":null,
                "application":"fenix",
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
            }),
            json!({
                "schemaVersion": "1.0.0",
                "slug": "secure-silver",
                "endDate": null,
                "branches":[
                    {"slug": "control", "ratio": 1},
                    {"slug": "treatment","ratio":1}
                ],
                "probeSets":[],
                "startDate":null,
                "application":"fenix",
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
            }),
        ]
    }

    fn insert_experiments(
        db: &Database,
        writer: &mut Writer,
        exps: Vec<serde_json::Value>,
    ) -> Result<()> {
        let store = db.get_store(StoreId::Experiments);
        store.clear(writer)?;
        for exp in exps {
            store.put(writer, exp.get("slug").unwrap().as_str().unwrap(), &exp)?;
        }
        Ok(())
    }

    fn get_experiment_enrollments(db: &Database) -> Result<Vec<ExperimentEnrollment>> {
        db.collect_all::<ExperimentEnrollment>(StoreId::Enrollments)
    }

    fn get_experiment_enrollments_with_status(
        db: &Database,
        status: EnrollmentStatus,
    ) -> Result<Vec<ExperimentEnrollment>> {
        let enrollments = get_experiment_enrollments(db)?;
        Ok(enrollments
            .into_iter()
            .filter(|enr| enr.status == status)
            .collect())
    }

    #[test]
    fn test_enrollments() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("test_enrollments")?;
        let db = Database::new(&tmp_dir)?;
        let exp = &get_test_experiments()[0];
        let nimbus_id = Uuid::new_v4();
        let aru = Default::default();
        assert_eq!(get_enrollments(&db)?.len(), 0);
        let mut writer = db.write()?;
        db.get_store(StoreId::Experiments).put(
            &mut writer,
            exp.get("slug").unwrap().as_str().unwrap(),
            exp,
        )?;

        update_enrollments(&db, &mut writer, &nimbus_id, &aru, &Default::default())?;
        writer.commit()?;

        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 1);
        let enrollment = &enrollments[0];
        assert_eq!(enrollment.slug, "secure-gold");
        assert_eq!(enrollment.user_facing_name, "Diagnostic test experiment");
        assert_eq!(
            enrollment.user_facing_description,
            "This is a test experiment for diagnostic purposes."
        );
        assert!(enrollment.branch_slug == "control" || enrollment.branch_slug == "treatment");

        // Get the ExperimentEnrollment from the DB.
        let ee = db
            .get::<ExperimentEnrollment>(StoreId::Enrollments, "secure-gold")?
            .expect("should exist");
        assert!(
            matches!(ee.status, EnrollmentStatus::Enrolled { reason: EnrolledReason::Qualified, .. })
        );

        // Now opt-out.
        opt_out(&db, "secure-gold")?;
        assert_eq!(get_enrollments(&db)?.len(), 0);
        // check we recorded the "why" correctly.
        let ee = db
            .get::<ExperimentEnrollment>(StoreId::Enrollments, "secure-gold")?
            .expect("should exist");
        assert_eq!(ee.status, EnrollmentStatus::OptedOut);

        // Opt in to a specific branch.
        opt_in_with_branch(&db, "secure-gold", "treatment")?;
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 1);
        let enrollment = &enrollments[0];
        assert_eq!(enrollment.slug, "secure-gold");
        assert!(enrollment.branch_slug == "treatment");
        Ok(())
    }

    #[test]
    fn test_updates() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("test_updates")?;
        let db = Database::new(&tmp_dir)?;
        let nimbus_id = Uuid::new_v4();
        let aru = Default::default();
        assert_eq!(get_enrollments(&db)?.len(), 0);
        let exps = get_test_experiments();
        let mut writer = db.write()?;
        insert_experiments(&db, &mut writer, exps)?;
        update_enrollments(&db, &mut writer, &nimbus_id, &aru, &Default::default())?;
        writer.commit()?;
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 2);

        let mut writer = db.write()?;
        let store = db.get_store(StoreId::Experiments);
        // pretend we just updated from the server and one of the 2 is missing.
        store.delete(&mut writer, "secure-gold")?;
        update_enrollments(&db, &mut writer, &nimbus_id, &aru, &Default::default())?;
        writer.commit()?;

        // should only have 1 now.
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 1);
        Ok(())
    }

    #[test]
    fn test_global_opt_out() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("test_global_opt_out")?;
        let db = Database::new(&tmp_dir)?;
        let mut writer = db.write()?;
        let nimbus_id = Uuid::new_v4();
        let aru = Default::default();
        let app_context = Default::default();
        assert_eq!(get_enrollments(&db)?.len(), 0);
        let exps = get_test_experiments();
        insert_experiments(&db, &mut writer, exps)?;

        // User has opted out of new experiments.
        // New experiments exist, but no enrolments have happened.
        set_global_user_participation(&db, &mut writer, false)?;
        update_enrollments(&db, &mut writer, &nimbus_id, &aru, &app_context)?;
        writer.commit()?;

        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 0);
        // We should see no experiment enrolments.
        assert_eq!(get_experiment_enrollments(&db)?.len(), 0);
        assert_eq!(
            get_experiment_enrollments_with_status(&db, EnrollmentStatus::OptedOut)?.len(),
            0
        );

        // User opts in, and updating should enrol us in 2 experiments.
        let mut writer = db.write()?;
        set_global_user_participation(&db, &mut writer, true)?;
        update_enrollments(&db, &mut writer, &nimbus_id, &aru, &app_context)?;
        writer.commit()?;
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 2);
        // We should see 2 experiment enrolments.
        assert_eq!(get_experiment_enrollments(&db)?.len(), 2);
        assert_eq!(
            get_experiment_enrollments_with_status(&db, EnrollmentStatus::OptedOut)?.len(),
            0
        );
        let branches: Vec<String> = enrollments
            .iter()
            .map(|exp| exp.branch_slug.clone())
            .collect();

        // Opting out and updating should give us no enrolled experiments,
        // but 2 experiment enrolments.
        let mut writer = db.write()?;
        set_global_user_participation(&db, &mut writer, false)?;
        update_enrollments(&db, &mut writer, &nimbus_id, &aru, &app_context)?;
        writer.commit()?;
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 0);
        // We should see 2 experiment enrolments, this time they're both opt outs
        assert_eq!(get_experiment_enrollments(&db)?.len(), 2);
        assert_eq!(
            get_experiment_enrollments_with_status(&db, EnrollmentStatus::OptedOut)?.len(),
            2
        );

        // Opting in again and updating should enrol us in 2 experiments.
        let mut writer = db.write()?;
        set_global_user_participation(&db, &mut writer, true)?;
        update_enrollments(&db, &mut writer, &nimbus_id, &aru, &app_context)?;
        writer.commit()?;
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 2);
        let new_branches: Vec<String> = enrollments
            .into_iter()
            .map(|exp| exp.branch_slug.clone())
            .collect();

        // Between opting in and out multiple times, branches remain stable.
        assert_eq!(branches, new_branches);

        // // pretend we just updated from the server and one of the 2 is missing.
        // db.delete(StoreId::Experiments, "secure-gold")?;
        // update_enrollments(&db, &nimbus_id, &aru, &Default::default())?;
        // // should only have 1 now.
        // let enrollments = get_enrollments(&db)?;
        // assert_eq!(enrollments.len(), 1);
        Ok(())
    }
}
