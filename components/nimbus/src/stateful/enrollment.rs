/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
use crate::{
    enrollment::{
        map_enrollments, EnrollmentChangeEvent, EnrollmentChangeEventType, EnrollmentsEvolver,
        ExperimentEnrollment,
    },
    error::{debug, warn, Result},
    stateful::{
        gecko_prefs::PrefUnenrollReason,
        persistence::{Database, Readable, StoreId, Writer},
    },
    EnrolledExperiment, EnrollmentStatus, Experiment,
};

const DB_KEY_GLOBAL_USER_PARTICIPATION: &str = "user-opt-in";
const DEFAULT_GLOBAL_USER_PARTICIPATION: bool = true;

impl EnrollmentsEvolver<'_> {
    /// Convenient wrapper around `evolve_enrollments` that fetches the current state of experiments,
    /// enrollments and user participation from the database.
    pub(crate) fn evolve_enrollments_in_db(
        &mut self,
        db: &Database,
        writer: &mut Writer,
        next_experiments: &[Experiment],
    ) -> Result<Vec<EnrollmentChangeEvent>> {
        // Get the state from the db.
        let is_user_participating = get_global_user_participation(db, writer)?;
        let experiments_store = db.get_store(StoreId::Experiments);
        let enrollments_store = db.get_store(StoreId::Enrollments);
        let prev_experiments: Vec<Experiment> = experiments_store.collect_all(writer)?;
        let prev_enrollments: Vec<ExperimentEnrollment> = enrollments_store.collect_all(writer)?;
        // Calculate the changes.
        let (next_enrollments, enrollments_change_events) = self.evolve_enrollments(
            is_user_participating,
            &prev_experiments,
            next_experiments,
            &prev_enrollments,
        )?;
        let next_enrollments = map_enrollments(&next_enrollments);
        // Write the changes to the Database.
        enrollments_store.clear(writer)?;
        for enrollment in next_enrollments.values() {
            enrollments_store.put(writer, &enrollment.slug, *enrollment)?;
        }
        experiments_store.clear(writer)?;
        for experiment in next_experiments {
            // Sanity check.
            if !next_enrollments.contains_key(&experiment.slug) {
                error_support::report_error!("nimbus-evolve-enrollments", "evolve_enrollments_in_db: experiment '{}' has no enrollment, dropping to keep database consistent", &experiment.slug);
                continue;
            }
            experiments_store.put(writer, &experiment.slug, experiment)?;
        }
        Ok(enrollments_change_events)
    }
}

/// Return information about all enrolled experiments.
/// Note this does not include rollouts
pub fn get_enrollments<'r>(
    db: &Database,
    reader: &'r impl Readable<'r>,
) -> Result<Vec<EnrolledExperiment>> {
    let enrollments: Vec<ExperimentEnrollment> =
        db.get_store(StoreId::Enrollments).collect_all(reader)?;
    let mut result = Vec::with_capacity(enrollments.len());
    for enrollment in enrollments {
        debug!("Have enrollment: {:?}", enrollment);
        if let EnrollmentStatus::Enrolled { branch, .. } = &enrollment.status {
            match db
                .get_store(StoreId::Experiments)
                .get::<Experiment, _>(reader, &enrollment.slug)?
            {
                Some(experiment) => {
                    result.push(EnrolledExperiment {
                        feature_ids: experiment.get_feature_ids(),
                        slug: experiment.slug,
                        user_facing_name: experiment.user_facing_name,
                        user_facing_description: experiment.user_facing_description,
                        branch_slug: branch.to_string(),
                    });
                }
                _ => {
                    warn!(
                        "Have enrollment {:?} but no matching experiment!",
                        enrollment
                    );
                }
            };
        }
    }
    Ok(result)
}

pub fn opt_in_with_branch(
    db: &Database,
    writer: &mut Writer,
    experiment_slug: &str,
    branch: &str,
) -> Result<Vec<EnrollmentChangeEvent>> {
    let mut events = vec![];
    if let Ok(Some(exp)) = db
        .get_store(StoreId::Experiments)
        .get::<Experiment, Writer>(writer, experiment_slug)
    {
        let enrollment = ExperimentEnrollment::from_explicit_opt_in(&exp, branch, &mut events);
        db.get_store(StoreId::Enrollments)
            .put(writer, experiment_slug, &enrollment.unwrap())?;
    } else {
        events.push(EnrollmentChangeEvent {
            experiment_slug: experiment_slug.to_string(),
            branch_slug: branch.to_string(),
            reason: Some("does-not-exist".to_string()),
            change: EnrollmentChangeEventType::EnrollFailed,
        });
    }

    Ok(events)
}

pub fn opt_out(
    db: &Database,
    writer: &mut Writer,
    experiment_slug: &str,
) -> Result<Vec<EnrollmentChangeEvent>> {
    let mut events = vec![];
    let enr_store = db.get_store(StoreId::Enrollments);
    if let Ok(Some(existing_enrollment)) =
        enr_store.get::<ExperimentEnrollment, Writer>(writer, experiment_slug)
    {
        let updated_enrollment = &existing_enrollment.on_explicit_opt_out(&mut events);
        enr_store.put(writer, experiment_slug, updated_enrollment)?;
    } else {
        events.push(EnrollmentChangeEvent {
            experiment_slug: experiment_slug.to_string(),
            branch_slug: "N/A".to_string(),
            reason: Some("does-not-exist".to_string()),
            change: EnrollmentChangeEventType::UnenrollFailed,
        });
    }

    Ok(events)
}

pub fn unenroll_for_pref(
    db: &Database,
    writer: &mut Writer,
    experiment_slug: &str,
    unenroll_reason: PrefUnenrollReason,
) -> Result<Vec<EnrollmentChangeEvent>> {
    let mut events = vec![];
    let enr_store = db.get_store(StoreId::Enrollments);
    if let Ok(Some(existing_enrollment)) =
        enr_store.get::<ExperimentEnrollment, Writer>(writer, experiment_slug)
    {
        let updated_enrollment =
            &existing_enrollment.on_pref_unenroll(unenroll_reason, &mut events);
        enr_store.put(writer, experiment_slug, updated_enrollment)?;
    } else {
        events.push(EnrollmentChangeEvent {
            experiment_slug: experiment_slug.to_string(),
            branch_slug: "N/A".to_string(),
            reason: Some("does-not-exist".to_string()),
            change: EnrollmentChangeEventType::UnenrollFailed,
        });
    }

    Ok(events)
}

pub fn get_global_user_participation<'r>(
    db: &Database,
    reader: &'r impl Readable<'r>,
) -> Result<bool> {
    let store = db.get_store(StoreId::Meta);
    let opted_in = store.get::<bool, _>(reader, DB_KEY_GLOBAL_USER_PARTICIPATION)?;
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

/// Reset unique identifiers in response to application-level telemetry reset.
///
pub fn reset_telemetry_identifiers(
    db: &Database,
    writer: &mut Writer,
) -> Result<Vec<EnrollmentChangeEvent>> {
    let mut events = vec![];
    let store = db.get_store(StoreId::Enrollments);
    let enrollments: Vec<ExperimentEnrollment> = store.collect_all(writer)?;
    let updated_enrollments = enrollments
        .iter()
        .map(|enrollment| enrollment.reset_telemetry_identifiers(&mut events));
    store.clear(writer)?;
    for enrollment in updated_enrollments {
        store.put(writer, &enrollment.slug, &enrollment)?;
    }
    Ok(events)
}
