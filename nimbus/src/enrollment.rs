// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
use crate::error::Result;
use crate::evaluator::evaluate_enrollment;
use crate::persistence::{Database, StoreId};
use crate::AvailableRandomizationUnits;
use crate::{AppContext, EnrolledExperiment, Experiment};

use ::uuid::Uuid;
use serde_derive::*;
use std::collections::{HashMap, HashSet};

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
    pub fn is_enrolled(&self) -> bool {
        match self {
            EnrollmentStatus::Enrolled { .. } => true,
            _ => false,
        }
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
    nimbus_id: &Uuid,
    aru: &AvailableRandomizationUnits,
    app_context: &AppContext,
) -> Result<()> {
    log::info!("updating enrollments...");
    // We might have enrollments for experiments which no longer exist, so we
    // first build a set of all IDs in both groups.
    let mut all_slugs = HashSet::new();
    let experiments = db.collect_all::<Experiment>(StoreId::Experiments)?;
    let mut map_experiments = HashMap::with_capacity(experiments.len());
    for e in experiments {
        all_slugs.insert(e.slug.clone());
        map_experiments.insert(e.slug.clone(), e);
    }
    // and existing enrollments.
    let enrollments = db.collect_all::<ExperimentEnrollment>(StoreId::Enrollments)?;
    let mut map_enrollments = HashMap::with_capacity(enrollments.len());
    for e in enrollments {
        all_slugs.insert(e.slug.clone());
        map_enrollments.insert(e.slug.clone(), e);
    }

    // XXX - we want to emit events for many of these things, but we are hoping
    // we can put that off until we have a glean rust sdk available and thus
    // avoid the complexity of passing the info to glean via kotlin/swift/js.
    for slug in all_slugs.iter() {
        match (map_experiments.get(slug), map_enrollments.get(slug)) {
            (Some(_), Some(enr)) => {
                // XXX - should check:
                // * is it still active?
                // * is enrollment was previously paused it may not be now.
                // * the branch still exists, etc?
                log::debug!("Experiment '{}' already has enrollment {:?}", slug, enr)
            }
            (Some(exp), None) => {
                let enr = evaluate_enrollment(nimbus_id, aru, app_context, &exp)?;
                log::debug!(
                    "Experiment '{}' is new - enrollment status is {:?}",
                    slug,
                    enr
                );
                db.put(StoreId::Enrollments, slug, &enr)?;
            }
            (None, Some(enr)) => {
                log::debug!(
                    "Experiment '{}' vanished while we had enrollment status of {:?}",
                    slug,
                    enr
                );
                db.delete(StoreId::Enrollments, slug)?;
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

/// Resets an experiment to remove any opt-in or opt-out overrides.
pub fn reset_enrollment(
    db: &Database,
    experiment_slug: &str,
    nimbus_id: &Uuid,
    aru: &AvailableRandomizationUnits,
    app_context: &AppContext,
) -> Result<()> {
    let exp = match db.get::<Experiment>(StoreId::Experiments, experiment_slug)? {
        None => {
            // XXX - do we want specific errors for this kind of thing?
            log::warn!("No such experiment '{}'", experiment_slug);
            return Ok(());
        }
        Some(e) => e,
    };
    let enrollment = evaluate_enrollment(nimbus_id, aru, app_context, &exp)?;
    db.put(StoreId::Enrollments, experiment_slug, &enrollment)
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
    db.put(StoreId::Enrollments, experiment_slug, &enrollment)
}

pub fn opt_out(db: &Database, experiment_slug: &str) -> Result<()> {
    // As above - check experiment exists?
    let enrollment = ExperimentEnrollment {
        slug: experiment_slug.to_string(),
        status: EnrollmentStatus::OptedOut,
    };
    db.put(StoreId::Enrollments, experiment_slug, &enrollment)
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

    #[test]
    fn test_enrollments() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("test_enrollments")?;
        let db = Database::new(&tmp_dir)?;
        let exp = &get_test_experiments()[0];
        let nimbus_id = Uuid::new_v4();
        let aru = Default::default();
        assert_eq!(get_enrollments(&db)?.len(), 0);
        db.put(
            StoreId::Experiments,
            exp.get("slug").unwrap().as_str().unwrap(),
            exp,
        )?;
        update_enrollments(&db, &nimbus_id, &aru, &Default::default())?;
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
        for exp in exps {
            db.put(
                StoreId::Experiments,
                exp.get("slug").unwrap().as_str().unwrap(),
                &exp,
            )?;
        }
        update_enrollments(&db, &nimbus_id, &aru, &Default::default())?;
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 2);
        // pretend we just updated from the server and one of the 2 is missing.
        db.delete(StoreId::Experiments, "secure-gold")?;
        update_enrollments(&db, &nimbus_id, &aru, &Default::default())?;
        // should only have 1 now.
        let enrollments = get_enrollments(&db)?;
        assert_eq!(enrollments.len(), 1);
        Ok(())
    }
}
