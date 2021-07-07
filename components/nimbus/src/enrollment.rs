// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
use crate::error::{NimbusError, Result};
use crate::persistence::{Database, StoreId, Writer};
use crate::{evaluator::evaluate_enrollment, persistence::Readable};
use crate::{
    AppContext, AvailableRandomizationUnits, EnrolledExperiment, Experiment, FeatureConfig,
};

use ::uuid::Uuid;
use serde_derive::*;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DB_KEY_GLOBAL_USER_PARTICIPATION: &str = "user-opt-in";
const DEFAULT_GLOBAL_USER_PARTICIPATION: bool = true;
const PREVIOUS_ENROLLMENTS_GC_TIME: Duration = Duration::from_secs(30 * 24 * 3600);

// These are types we use internally for managing enrollments.
// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum EnrolledReason {
    /// A normal enrollment as per the experiment's rules.
    Qualified,
    /// Explicit opt-in.
    OptIn,
}

// These are types we use internally for managing non-enrollments.

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum NotEnrolledReason {
    /// The user opted-out of experiments before we ever got enrolled to this one.
    OptOut,
    /// The evaluator bucketing did not choose us.
    NotSelected,
    /// We are not being targeted for this experiment.
    NotTargeted,
    /// The experiment enrollment is paused.
    EnrollmentsPaused,
    /// The experiment used a feature that was already under experiment.
    FeatureConflict,
}

// These are types we use internally for managing disqualifications.

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum DisqualifiedReason {
    /// There was an error.
    Error,
    /// The user opted-out from this experiment or experiments in general.
    OptOut,
    /// The targeting has changed for an experiment.
    NotTargeted,
}

// Every experiment has an ExperimentEnrollment, even when we aren't enrolled.

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct ExperimentEnrollment {
    pub slug: String,
    pub status: EnrollmentStatus,
}

impl ExperimentEnrollment {
    /// Evaluate an experiment enrollment for an experiment
    /// we are seeing for the first time.
    fn from_new_experiment(
        is_user_participating: bool,
        nimbus_id: &Uuid,
        available_randomization_units: &AvailableRandomizationUnits,
        app_context: &AppContext,
        experiment: &Experiment,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> Result<Self> {
        Ok(if !is_user_participating {
            Self {
                slug: experiment.slug.clone(),
                status: EnrollmentStatus::NotEnrolled {
                    reason: NotEnrolledReason::OptOut,
                },
            }
        } else if experiment.is_enrollment_paused {
            Self {
                slug: experiment.slug.clone(),
                status: EnrollmentStatus::NotEnrolled {
                    reason: NotEnrolledReason::EnrollmentsPaused,
                },
            }
        } else {
            let enrollment = evaluate_enrollment(
                nimbus_id,
                available_randomization_units,
                app_context,
                experiment,
            )?;
            log::debug!(
                "Experiment '{}' is new - enrollment status is {:?}",
                &enrollment.slug,
                &enrollment
            );
            if matches!(enrollment.status, EnrollmentStatus::Enrolled { .. }) {
                out_enrollment_events.push(enrollment.get_change_event())
            }
            enrollment
        })
    }

    /// Force enroll ourselves in an experiment.
    fn from_explicit_opt_in(
        experiment: &Experiment,
        branch_slug: &str,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> Result<Self> {
        if !experiment.has_branch(branch_slug) {
            return Err(NimbusError::NoSuchBranch(
                branch_slug.to_owned(),
                experiment.slug.clone(),
            ));
        }
        let enrollment = Self {
            slug: experiment.slug.clone(),
            status: EnrollmentStatus::new_enrolled(EnrolledReason::OptIn, branch_slug),
        };
        out_enrollment_events.push(enrollment.get_change_event());
        Ok(enrollment)
    }

    /// Update our enrollment to an experiment we have seen before.
    fn on_experiment_updated(
        &self,
        is_user_participating: bool,
        nimbus_id: &Uuid,
        available_randomization_units: &AvailableRandomizationUnits,
        app_context: &AppContext,
        updated_experiment: &Experiment,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> Result<Self> {
        Ok(match self.status {
            EnrollmentStatus::NotEnrolled { .. } => {
                if !is_user_participating || updated_experiment.is_enrollment_paused {
                    self.clone()
                } else {
                    let updated_enrollment = evaluate_enrollment(
                        nimbus_id,
                        available_randomization_units,
                        app_context,
                        updated_experiment,
                    )?;
                    log::debug!(
                        "Experiment '{}' with enrollment {:?} is now {:?}",
                        &self.slug,
                        &self,
                        updated_enrollment
                    );
                    if matches!(updated_enrollment.status, EnrollmentStatus::Enrolled { .. }) {
                        out_enrollment_events.push(updated_enrollment.get_change_event());
                    }
                    updated_enrollment
                }
            }
            EnrollmentStatus::Enrolled {
                ref branch,
                ref reason,
                ..
            } => {
                if !is_user_participating {
                    log::debug!(
                        "Existing experiment enrollment '{}' is now disqualified (global opt-out)",
                        &self.slug
                    );
                    let updated_enrollment =
                        self.disqualify_from_enrolled(DisqualifiedReason::OptOut);
                    out_enrollment_events.push(updated_enrollment.get_change_event());
                    updated_enrollment
                } else if !updated_experiment.has_branch(branch) {
                    // The branch we were in disappeared!
                    let updated_enrollment =
                        self.disqualify_from_enrolled(DisqualifiedReason::Error);
                    out_enrollment_events.push(updated_enrollment.get_change_event());
                    updated_enrollment
                } else if matches!(reason, EnrolledReason::OptIn) {
                    // we check if we opted-in an experiment, if so
                    // we don't need to update our enrollment
                    self.clone()
                } else {
                    let evaluated_enrollment = evaluate_enrollment(
                        nimbus_id,
                        available_randomization_units,
                        app_context,
                        updated_experiment,
                    )?;
                    match evaluated_enrollment.status {
                        EnrollmentStatus::Error { .. } => {
                            let updated_enrollment =
                                self.disqualify_from_enrolled(DisqualifiedReason::Error);
                            out_enrollment_events.push(updated_enrollment.get_change_event());
                            updated_enrollment
                        }
                        EnrollmentStatus::NotEnrolled {
                            reason: NotEnrolledReason::NotTargeted,
                        } => {
                            log::debug!("Existing experiment enrollment '{}' is now disqualified (targeting change)", &self.slug);
                            let updated_enrollment =
                                self.disqualify_from_enrolled(DisqualifiedReason::NotTargeted);
                            out_enrollment_events.push(updated_enrollment.get_change_event());
                            updated_enrollment
                        }
                        EnrollmentStatus::NotEnrolled { .. }
                        | EnrollmentStatus::Enrolled { .. }
                        | EnrollmentStatus::Disqualified { .. }
                        | EnrollmentStatus::WasEnrolled { .. } => self.clone(),
                    }
                }
            }
            EnrollmentStatus::Disqualified {
                ref branch,
                enrollment_id,
                ..
            } => {
                if !is_user_participating {
                    log::debug!(
                        "Disqualified experiment enrollment '{}' has been reset to not-enrolled (global opt-out)",
                        &self.slug
                    );
                    Self {
                        slug: self.slug.clone(),
                        status: EnrollmentStatus::Disqualified {
                            reason: DisqualifiedReason::OptOut,
                            enrollment_id,
                            branch: branch.clone(),
                        },
                    }
                } else {
                    self.clone()
                }
            }
            EnrollmentStatus::WasEnrolled { .. } | EnrollmentStatus::Error { .. } => self.clone(), // Cannot recover from errors!
        })
    }

    /// Transition our enrollment to WasEnrolled (Option::Some) or delete it (Option::None)
    /// after an experiment has disappeared from the server.
    ///
    /// If we transitioned to WasEnrolled, our enrollment will be garbage collected
    /// from the database after `PREVIOUS_ENROLLMENTS_GC_TIME`.
    fn on_experiment_ended(
        &self,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> Option<Self> {
        log::debug!(
            "Experiment '{}' vanished while we had enrollment status of {:?}",
            self.slug,
            self
        );
        let (branch, enrollment_id) = match self.status {
            EnrollmentStatus::Enrolled {
                ref branch,
                enrollment_id,
                ..
            } => (branch, enrollment_id),
            EnrollmentStatus::Disqualified {
                ref branch,
                enrollment_id,
                ..
            } => (branch, enrollment_id),
            EnrollmentStatus::NotEnrolled { .. }
            | EnrollmentStatus::WasEnrolled { .. }
            | EnrollmentStatus::Error { .. } => return None, // We were never enrolled anyway, simply delete the enrollment record from the DB.
        };
        let enrollment = Self {
            slug: self.slug.clone(),
            status: EnrollmentStatus::WasEnrolled {
                enrollment_id,
                branch: branch.to_owned(),
                experiment_ended_at: now_secs(),
            },
        };
        out_enrollment_events.push(enrollment.get_change_event());
        Some(enrollment)
    }

    /// Force unenroll ourselves from an experiment.
    #[allow(clippy::unnecessary_wraps)]
    fn on_explicit_opt_out(
        &self,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> ExperimentEnrollment {
        match self.status {
            EnrollmentStatus::Enrolled { .. } => {
                let enrollment = self.disqualify_from_enrolled(DisqualifiedReason::OptOut);
                out_enrollment_events.push(enrollment.get_change_event());
                enrollment
            }
            EnrollmentStatus::NotEnrolled { .. } => Self {
                slug: self.slug.to_string(),
                status: EnrollmentStatus::NotEnrolled {
                    reason: NotEnrolledReason::OptOut, // Explicitly set the reason to OptOut.
                },
            },
            EnrollmentStatus::Disqualified { .. }
            | EnrollmentStatus::WasEnrolled { .. }
            | EnrollmentStatus::Error { .. } => {
                // Nothing to do here.
                self.clone()
            }
        }
    }

    /// Reset identifiers in response to application-level telemetry reset.
    ///
    /// Each experiment enrollment record contains a unique `enrollment_id`. When the user
    /// resets their application-level telemetry, we reset each such id to a special nil value,
    /// creating a clean break between data sent before the reset and any data that might be
    /// submitted about these enrollments in future.
    ///
    /// We also move any enrolled experiments to the "disqualified" state, since their further
    /// partipation would submit partial data that could skew analysis.
    ///
    fn reset_telemetry_identifiers(
        &self,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> Self {
        let updated = match self.status {
            EnrollmentStatus::Enrolled { .. } => {
                let disqualified = self.disqualify_from_enrolled(DisqualifiedReason::OptOut);
                out_enrollment_events.push(disqualified.get_change_event());
                disqualified
            }
            EnrollmentStatus::NotEnrolled { .. }
            | EnrollmentStatus::Disqualified { .. }
            | EnrollmentStatus::WasEnrolled { .. }
            | EnrollmentStatus::Error { .. } => self.clone(),
        };
        ExperimentEnrollment {
            status: updated.status.clone_with_nil_enrollment_id(),
            ..updated
        }
    }

    /// Garbage collect old experiments we've kept a WasEnrolled enrollment from.
    /// Returns Option::None if the enrollment should be nuked from the db.
    fn maybe_garbage_collect(&self) -> Option<Self> {
        if let EnrollmentStatus::WasEnrolled {
            experiment_ended_at,
            ..
        } = self.status
        {
            let time_since_transition = Duration::from_secs(now_secs() - experiment_ended_at);
            if time_since_transition < PREVIOUS_ENROLLMENTS_GC_TIME {
                return Some(self.clone());
            }
        }
        log::debug!("Garbage collecting enrollment '{}'", self.slug);
        None
    }

    // Create a telemetry event describing the transition
    // to the current enrollment state.
    fn get_change_event(&self) -> EnrollmentChangeEvent {
        match &self.status {
            EnrollmentStatus::Enrolled {
                enrollment_id,
                branch,
                ..
            } => EnrollmentChangeEvent::new(
                &self.slug,
                enrollment_id,
                branch,
                None,
                EnrollmentChangeEventType::Enrollment,
            ),
            EnrollmentStatus::WasEnrolled {
                enrollment_id,
                branch,
                ..
            } => EnrollmentChangeEvent::new(
                &self.slug,
                &enrollment_id,
                &branch,
                None,
                EnrollmentChangeEventType::Unenrollment,
            ),
            EnrollmentStatus::Disqualified {
                enrollment_id,
                branch,
                reason,
                ..
            } => EnrollmentChangeEvent::new(
                &self.slug,
                &enrollment_id,
                &branch,
                match reason {
                    DisqualifiedReason::NotTargeted => Some("targeting"),
                    DisqualifiedReason::OptOut => Some("optout"),
                    DisqualifiedReason::Error => Some("error"),
                },
                EnrollmentChangeEventType::Disqualification,
            ),
            EnrollmentStatus::NotEnrolled { .. } | EnrollmentStatus::Error { .. } => unreachable!(),
        }
    }

    /// If the current state is `Enrolled`, move to `Disqualified` with the given reason.
    fn disqualify_from_enrolled(&self, reason: DisqualifiedReason) -> Self {
        match self.status {
            EnrollmentStatus::Enrolled {
                ref enrollment_id,
                ref branch,
                ..
            } => ExperimentEnrollment {
                status: EnrollmentStatus::Disqualified {
                    reason,
                    enrollment_id: enrollment_id.to_owned(),
                    branch: branch.to_owned(),
                },
                ..self.clone()
            },
            EnrollmentStatus::NotEnrolled { .. }
            | EnrollmentStatus::Disqualified { .. }
            | EnrollmentStatus::WasEnrolled { .. }
            | EnrollmentStatus::Error { .. } => self.clone(),
        }
    }
}

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum EnrollmentStatus {
    Enrolled {
        enrollment_id: Uuid, // Random ID used for telemetry events correlation.
        reason: EnrolledReason,
        branch: String,
    },
    NotEnrolled {
        reason: NotEnrolledReason,
    },
    Disqualified {
        enrollment_id: Uuid,
        reason: DisqualifiedReason,
        branch: String,
    },
    WasEnrolled {
        enrollment_id: Uuid,
        branch: String,
        experiment_ended_at: u64, // unix timestamp in sec, used to GC old enrollments
    },
    // There was some error opting in.
    Error {
        // Ideally this would be an Error, but then we'd need to make Error
        // serde compatible, which isn't trivial nor desirable.
        reason: String,
    },
}

impl EnrollmentStatus {
    // Note that for now, we only support a single feature_id per experiment,
    // so this code is expected to shift once we start supporting multiple.
    pub fn new_enrolled(reason: EnrolledReason, branch: &str) -> Self {
        EnrollmentStatus::Enrolled {
            reason,
            branch: branch.to_owned(),
            enrollment_id: Uuid::new_v4(),
        }
    }

    // This is used in examples, but not in the main dylib, and
    // triggers a dead code warning when building with `--release`.
    #[allow(dead_code)]
    pub fn is_enrolled(&self) -> bool {
        matches!(self, EnrollmentStatus::Enrolled { .. })
    }

    /// Make a clone of this status, but with the special nil enrollment_id.
    fn clone_with_nil_enrollment_id(&self) -> Self {
        let mut updated = self.clone();
        match updated {
            EnrollmentStatus::Enrolled {
                ref mut enrollment_id,
                ..
            }
            | EnrollmentStatus::Disqualified {
                ref mut enrollment_id,
                ..
            }
            | EnrollmentStatus::WasEnrolled {
                ref mut enrollment_id,
                ..
            } => *enrollment_id = Uuid::nil(),
            EnrollmentStatus::NotEnrolled { .. } | EnrollmentStatus::Error { .. } => (),
        };
        updated
    }
}

/// Return information about all enrolled experiments.
pub fn get_enrollments<'r>(
    db: &Database,
    reader: &'r impl Readable<'r>,
) -> Result<Vec<EnrolledExperiment>> {
    let enrollments: Vec<ExperimentEnrollment> =
        db.get_store(StoreId::Enrollments).collect_all(reader)?;
    let mut result = Vec::with_capacity(enrollments.len());
    for enrollment in enrollments {
        log::debug!("Have enrollment: {:?}", enrollment);
        if let EnrollmentStatus::Enrolled {
            branch,
            enrollment_id,
            ..
        } = &enrollment.status
        {
            if let Some(experiment) = db
                .get_store(StoreId::Experiments)
                .get::<Experiment, _>(reader, &enrollment.slug)?
            {
                result.push(EnrolledExperiment {
                    feature_ids: experiment.get_feature_ids(),
                    slug: experiment.slug,
                    user_facing_name: experiment.user_facing_name,
                    user_facing_description: experiment.user_facing_description,
                    branch_slug: branch.to_string(),
                    enrollment_id: enrollment_id.to_string(),
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

pub(crate) struct EnrollmentsEvolver<'a> {
    nimbus_id: &'a Uuid,
    available_randomization_units: &'a AvailableRandomizationUnits,
    app_context: &'a AppContext,
}

impl<'a> EnrollmentsEvolver<'a> {
    pub(crate) fn new(
        nimbus_id: &'a Uuid,
        available_randomization_units: &'a AvailableRandomizationUnits,
        app_context: &'a AppContext,
    ) -> Self {
        Self {
            nimbus_id,
            available_randomization_units,
            app_context,
        }
    }

    /// Convenient wrapper around `evolve_enrollments` that fetches the current state of experiments,
    /// enrollments and user participation from the database.
    pub(crate) fn evolve_enrollments_in_db(
        &self,
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
                log::error!("evolve_enrollments_in_db: experiment '{}' has no enrollment, dropping to keep database consistent", &experiment.slug);
                continue;
            }
            experiments_store.put(writer, &experiment.slug, experiment)?;
        }
        Ok(enrollments_change_events)
    }

    /// Evolve and calculate the new set of enrollments, using the
    /// previous and current state of experiments and current enrollments.
    pub(crate) fn evolve_enrollments(
        &self,
        is_user_participating: bool,
        prev_experiments: &[Experiment],
        next_experiments: &[Experiment],
        prev_enrollments: &[ExperimentEnrollment],
    ) -> Result<(Vec<ExperimentEnrollment>, Vec<EnrollmentChangeEvent>)> {
        let mut enrollment_events = vec![];
        let prev_experiments = map_experiments(&prev_experiments);
        let next_experiments = map_experiments(&next_experiments);
        let prev_enrollments = map_enrollments(&prev_enrollments);

        // Step 1. Build an initial active_features to keep track of
        // the features that are being experimented upon.
        let mut enrolled_features = HashMap::with_capacity(next_experiments.len());

        let mut next_enrollments = Vec::with_capacity(next_experiments.len());

        // Step 2.
        // Evolve the experiments with previous enrollments first (except for
        // those that already have a feature conflict).  While we're doing so,
        // start building up active_features, the map of feature_ids under
        // experiment to EnrolledFeatureConfigs, and next_enrollments.

        for prev_enrollment in prev_enrollments.values() {
            if matches!(
                prev_enrollment.status,
                EnrollmentStatus::NotEnrolled {
                    reason: NotEnrolledReason::FeatureConflict
                }
            ) {
                continue;
            }
            let slug = &prev_enrollment.slug;

            let next_enrollment = match self.evolve_enrollment(
                is_user_participating,
                prev_experiments.get(slug).copied(),
                next_experiments.get(slug).copied(),
                Some(prev_enrollment),
                &mut enrollment_events,
            ) {
                Ok(enrollment) => enrollment,
                Err(e) => {
                    // It would be a fine thing if we had counters that
                    // collected the number of errors here, and at the
                    // place in this function where enrollments could be
                    // dropped.  We could then send those errors to
                    // telemetry so that they could be monitored (SDK-309)
                    log::error!("evolve_enrollment(\n\tprev_exp: {:?}\n\t, next_exp: {:?}, \n\tprev_enrollment: {:?})\n\t returned an error: {}; dropping this record", prev_experiments.get(slug).copied(), Some(next_experiments.get(slug).copied()), prev_enrollment, e);
                    None
                }
            };

            self.reserve_enrolled_features(
                next_enrollment,
                &next_experiments,
                &mut enrolled_features,
                &mut next_enrollments,
            );
        }

        // Step 3. Evolve the remaining enrollments with the previous and
        // next data.
        for next_experiment in next_experiments.values() {
            let slug = &next_experiment.slug;

            // Check that the feature ids that this experiment needs are available.  If not, then declare
            // the enrollment as NotEnrolled; and we continue to the next
            // experiment.
            // `needed_features_in_use` are the features needed for this experiment, but already in use.
            // If this is not empty, then the experiment is either already enrolled, or cannot be enrolled.
            let needed_features_in_use: Vec<&EnrolledFeatureConfig> = next_experiment
                .get_feature_ids()
                .iter()
                .filter_map(|id| enrolled_features.get(id))
                .collect();
            if !needed_features_in_use.is_empty() {
                let is_our_experiment = needed_features_in_use.iter().any(|f| &f.slug == slug);
                if is_our_experiment {
                    // At least one of these conflicted features are in use by this experiment.
                    // Unless the experiment has changed midflight, all the features will be from
                    // this experiment.
                    assert!(needed_features_in_use.iter().all(|f| &f.slug == slug));
                    // N.B. If this experiment is enrolled already, then we called
                    // evolve_enrollment() on this enrollment and this experiment above.
                } else {
                    // At least one feature needed for this experiment is already in use by another experiment.
                    // Thus, we cannot proceed with an enrollment other than as a `FeatureConflict`.
                    next_enrollments.push(ExperimentEnrollment {
                        slug: slug.clone(),
                        status: EnrollmentStatus::NotEnrolled {
                            reason: NotEnrolledReason::FeatureConflict,
                        },
                    });
                }
                // Whether it's our experiment or not that is using these features, no further enrollment can
                // happen.
                // Because no change has happened to this experiment's enrollment status, we don't need
                // to log an enrollment event.
                // All we can do is continue to the next experiment.
                continue;
            }

            // If we got here, then the features are not already active.
            // But we evolved all the existing enrollments in step 2,
            // (except the feature conflicted ones)
            // so we should be mindful that we don't evolve them a second time.
            let prev_enrollment = prev_enrollments.get(slug).copied();

            if prev_enrollment.is_none()
                || matches!(
                    prev_enrollment.unwrap().status,
                    EnrollmentStatus::NotEnrolled {
                        reason: NotEnrolledReason::FeatureConflict
                    }
                )
            {
                let next_enrollment = match self.evolve_enrollment(
                    is_user_participating,
                    prev_experiments.get(slug).copied(),
                    Some(next_experiment),
                    prev_enrollment,
                    &mut enrollment_events,
                ) {
                    Ok(enrollment) => enrollment,
                    Err(e) => {
                        // It would be a fine thing if we had counters that
                        // collected the number of errors here, and at the
                        // place in this function where enrollments could be
                        // dropped.  We could then send those errors to
                        // telemetry so that they could be monitored (SDK-309)
                        log::error!("evolve_enrollment(\n\tprev_exp: {:?}\n\t, next_exp: {:?}, \n\tprev_enrollment: {:?})\n\t returned an error: {}; dropping this record", prev_experiments.get(slug).copied(), Some(next_experiment), prev_enrollment, e);
                        None
                    }
                };

                self.reserve_enrolled_features(
                    next_enrollment,
                    &next_experiments,
                    &mut enrolled_features,
                    &mut next_enrollments,
                );
            }
        }

        // Check that we generate the enrolled feature map from the new
        // enrollments and new experiments.  Perhaps this should just be an
        // assert.
        let updated_enrolled_features = map_features(&next_enrollments, &next_experiments);
        if enrolled_features != updated_enrolled_features {
            Err(NimbusError::InternalError(
                "Next enrollment calculation error",
            ))
        } else {
            Ok((next_enrollments, enrollment_events))
        }
    }

    // Book-keeping method used in evolve_enrollments.
    fn reserve_enrolled_features(
        &self,
        latest_enrollment: Option<ExperimentEnrollment>,
        experiments: &HashMap<String, &Experiment>,
        enrolled_features: &mut HashMap<String, EnrolledFeatureConfig>,
        enrollments: &mut Vec<ExperimentEnrollment>,
    ) {
        if let Some(enrollment) = latest_enrollment {
            // Now we have an enrollment object!
            // If it's an enrolled enrollment, then get the FeatureConfigs
            // from the experiment and store them in the active_features map.
            for enrolled_feature in get_enrolled_feature_configs(&enrollment, &experiments) {
                enrolled_features.insert(enrolled_feature.feature_id.clone(), enrolled_feature);
            }
            // Also, record the enrollment for our return value
            enrollments.push(enrollment);
        }
    }

    /// Evolve a single enrollment using the previous and current state of an
    /// experiment and maybe garbage collect at least a subset of invalid
    /// experiments.
    ///
    /// XXX need to verify the exact set of gc-related side-effects and
    /// document them here.
    ///
    /// Returns an Option-wrapped version of the updated enrollment.  None
    /// means that the enrollment has been/should be discarded.
    fn evolve_enrollment(
        &self,
        is_user_participating: bool,
        prev_experiment: Option<&Experiment>,
        next_experiment: Option<&Experiment>,
        prev_enrollment: Option<&ExperimentEnrollment>,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>, // out param containing the events we'd like to emit to glean.
    ) -> Result<Option<ExperimentEnrollment>> {
        Ok(match (prev_experiment, next_experiment, prev_enrollment) {
            // New experiment.
            (None, Some(experiment), None) => Some(ExperimentEnrollment::from_new_experiment(
                is_user_participating,
                self.nimbus_id,
                self.available_randomization_units,
                self.app_context,
                experiment,
                out_enrollment_events,
            )?),
            // Experiment deleted remotely.
            (Some(_), None, Some(enrollment)) => {
                enrollment.on_experiment_ended(out_enrollment_events)
            }
            // Known experiment.
            (Some(_), Some(experiment), Some(enrollment)) => {
                Some(enrollment.on_experiment_updated(
                    is_user_participating,
                    self.nimbus_id,
                    self.available_randomization_units,
                    self.app_context,
                    experiment,
                    out_enrollment_events,
                )?)
            }
            (None, None, Some(enrollment)) => enrollment.maybe_garbage_collect(),
            (None, Some(_), Some(_)) => {
                return Err(NimbusError::InternalError(
                    "New experiment but enrollment already exists.",
                ))
            }
            (Some(_), None, None) | (Some(_), Some(_), None) => {
                return Err(NimbusError::InternalError(
                    "Experiment in the db did not have an associated enrollment record.",
                ))
            }
            (None, None, None) => {
                return Err(NimbusError::InternalError(
                    "evolve_experiment called with nothing that could evolve or be evolved",
                ))
            }
        })
    }
}

fn map_experiments(experiments: &[Experiment]) -> HashMap<String, &Experiment> {
    let mut map_experiments = HashMap::with_capacity(experiments.len());
    for e in experiments {
        map_experiments.insert(e.slug.clone(), e);
    }
    map_experiments
}

fn map_enrollments(enrollments: &[ExperimentEnrollment]) -> HashMap<String, &ExperimentEnrollment> {
    let mut map_enrollments = HashMap::with_capacity(enrollments.len());
    for e in enrollments {
        map_enrollments.insert(e.slug.clone(), e);
    }
    map_enrollments
}

/// Take a list of enrollments and a map of experiments, and generate mapping of `feature_id` to
/// `EnrolledFeatureConfig` structs.
fn map_features(
    enrollments: &[ExperimentEnrollment],
    experiments: &HashMap<String, &Experiment>,
) -> HashMap<String, EnrolledFeatureConfig> {
    let mut map = HashMap::with_capacity(enrollments.len());
    for enrolled_feature_config in enrollments
        .iter()
        .flat_map(|e| get_enrolled_feature_configs(e, experiments))
    {
        map.insert(
            enrolled_feature_config.feature_id.clone(),
            enrolled_feature_config,
        );
    }

    map
}

pub fn map_features_by_feature_id(
    enrollments: &[ExperimentEnrollment],
    experiments: &[Experiment],
) -> HashMap<String, EnrolledFeatureConfig> {
    map_features(enrollments, &map_experiments(experiments))
}

fn get_enrolled_feature_configs(
    enrollment: &ExperimentEnrollment,
    experiments: &HashMap<String, &Experiment>,
) -> Vec<EnrolledFeatureConfig> {
    // If status is not enrolled, then we can leave early.
    let branch_slug = match &enrollment.status {
        EnrollmentStatus::Enrolled { branch, .. } => branch,
        _ => return Vec::new(),
    };

    let experiment_slug = &enrollment.slug;

    let experiment = match experiments.get(experiment_slug).copied() {
        Some(exp) => exp,
        _ => return Vec::new(),
    };

    // Get the branch from the experiment, and then get the feature configs
    // from there.
    let branch_features = match &experiment.get_branch(branch_slug) {
        Some(branch) => branch.get_feature_configs(),
        _ => Default::default(),
    };

    let branch_feature_ids = branch_features
        .iter()
        .map(|f| &f.feature_id)
        .collect::<HashSet<_>>();

    // The experiment might have other branches that deal with different features.
    // We don't want them getting involved in other experiments, so we'll make default
    // FeatureConfigs.
    let non_branch_features: Vec<FeatureConfig> = experiment
        .get_feature_ids()
        .into_iter()
        .filter(|feature_id| !branch_feature_ids.contains(feature_id))
        .map(|feature_id| FeatureConfig {
            feature_id,
            ..Default::default()
        })
        .collect();

    // Now we've got the feature configs for all features in this experiment,
    // we can make EnrolledFeatureConfigs with them.
    branch_features
        .iter()
        .chain(non_branch_features.iter())
        .map(|f| EnrolledFeatureConfig {
            feature: f.to_owned(),
            slug: experiment_slug.clone(),
            branch: branch_slug.clone(),
            feature_id: f.feature_id.clone(),
        })
        .collect()
}

/// Small transitory struct to contain all the information needed to configure a feature with the Feature API.
/// By design, we don't want to store it on the disk. Instead we calculate it from experiments
/// and enrollments.
#[derive(Debug, Clone, PartialEq)]
pub struct EnrolledFeatureConfig {
    pub feature: FeatureConfig,
    pub slug: String,
    pub branch: String,
    pub feature_id: String,
}

#[derive(Debug)]
pub struct EnrollmentChangeEvent {
    pub experiment_slug: String,
    pub branch_slug: String,
    pub enrollment_id: String,
    pub reason: Option<String>,
    pub change: EnrollmentChangeEventType,
}

impl EnrollmentChangeEvent {
    pub(crate) fn new(
        slug: &str,
        enrollment_id: &Uuid,
        branch: &str,
        reason: Option<&str>,
        change: EnrollmentChangeEventType,
    ) -> Self {
        Self {
            experiment_slug: slug.to_owned(),
            branch_slug: branch.to_owned(),
            reason: reason.map(|s| s.to_owned()),
            enrollment_id: enrollment_id.to_string(),
            change,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum EnrollmentChangeEventType {
    Enrollment,
    Disqualification,
    Unenrollment,
}

pub fn opt_in_with_branch(
    db: &Database,
    writer: &mut Writer,
    experiment_slug: &str,
    branch: &str,
) -> Result<Vec<EnrollmentChangeEvent>> {
    let mut events = vec![];
    let exp: Experiment = db
        .get_store(StoreId::Experiments)
        .get(writer, experiment_slug)?
        .ok_or_else(|| NimbusError::NoSuchExperiment(experiment_slug.to_owned()))?;
    let enrollment = ExperimentEnrollment::from_explicit_opt_in(&exp, branch, &mut events)?;
    db.get_store(StoreId::Enrollments)
        .put(writer, experiment_slug, &enrollment)?;
    Ok(events)
}

pub fn opt_out(
    db: &Database,
    writer: &mut Writer,
    experiment_slug: &str,
) -> Result<Vec<EnrollmentChangeEvent>> {
    let mut events = vec![];
    let enr_store = db.get_store(StoreId::Enrollments);
    let existing_enrollment: ExperimentEnrollment = enr_store
        .get(writer, experiment_slug)?
        .ok_or_else(|| NimbusError::NoSuchExperiment(experiment_slug.to_owned()))?;
    let updated_enrollment = existing_enrollment.on_explicit_opt_out(&mut events);
    enr_store.put(writer, experiment_slug, &updated_enrollment)?;
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

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current date before Unix Epoch.")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{Database, StoreId};
    use serde_json::json;
    use tempdir::TempDir;

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
        app_ctx: &'a AppContext,
        aru: &'a AvailableRandomizationUnits,
    ) -> EnrollmentsEvolver<'a> {
        EnrollmentsEvolver::new(nimbus_id, aru, app_ctx)
    }

    #[test]
    fn test_evolver_new_experiment_enrolled() -> Result<()> {
        let exp = &get_test_experiments()[0];
        let (nimbus_id, app_ctx, aru) = local_ctx();
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
            },
            crate::Branch {
                slug: "bobo-branch".to_owned(),
                ratio: 1,
                feature: None,
            },
        ];
        let (nimbus_id, app_ctx, aru) = local_ctx();
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        }];
        let (nimbus_id, app_ctx, aru) = local_ctx();
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);

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
        let enrolled: Vec<ExperimentEnrollment> = enrollments
            .clone()
            .into_iter()
            .filter(|e| matches!(e.status, EnrollmentStatus::WasEnrolled { .. }))
            .collect();
        assert_eq!(1, enrolled.len());

        let enrolled: Vec<ExperimentEnrollment> = enrollments
            .into_iter()
            .filter(|e| matches!(e.status, EnrollmentStatus::Enrolled { .. }))
            .collect();
        assert_eq!(0, enrolled.len());

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
        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
        let (enrollments, events) =
            evolver.evolve_enrollments(true, &[], &test_experiments, &[])?;

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
            2,
            events.len(),
            "There should be exactly 2 enrollment_change_events"
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
        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
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
        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);

        let aboutwelcome_experiment = get_experiment_with_aboutwelcome_feature_branches();
        let newtab_experiment = get_experiment_with_newtab_feature_branches();
        let mixed_experiment = get_experiment_with_different_feature_branches();

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

        assert_eq!(events.len(), 0);

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
        let next_experiments = vec![aboutwelcome_experiment, newtab_experiment, mixed_experiment];
        let (enrollments, events) = evolver.evolve_enrollments(
            true,
            &prev_experiments,
            &next_experiments,
            &prev_enrollments,
        )?;

        assert_eq!(events.len(), 0);
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

        Ok(())
    }

    #[test]
    fn test_evolver_experiment_update_was_enrolled() -> Result<()> {
        let exp = get_test_experiments()[0].clone();
        let (nimbus_id, app_ctx, aru) = local_ctx();
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);

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
    fn test_evolver_experiment_update_error() -> Result<()> {
        let exp = get_test_experiments()[0].clone();
        let (nimbus_id, app_ctx, aru) = local_ctx();
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
        let mut events = vec![];
        let existing_enrollment = ExperimentEnrollment {
            slug: exp.slug.clone(),
            status: EnrollmentStatus::Error {
                reason: "heh".to_owned(),
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
    fn test_evolver_experiment_ended_was_enrolled() -> Result<()> {
        let exp = get_test_experiments()[0].clone();
        let (nimbus_id, app_ctx, aru) = local_ctx();
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
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
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
        let res = evolver.evolve_enrollment(true, Some(&exp), Some(&exp), None, &mut vec![]);
        assert!(res.is_err());
    }

    #[test]
    #[should_panic]
    fn test_evolver_no_experiments_no_enrollment() {
        let (nimbus_id, app_ctx, aru) = local_ctx();
        let evolver = enrollment_evolver(&nimbus_id, &app_ctx, &aru);
        evolver
            .evolve_enrollment(true, None, None, None, &mut vec![])
            .unwrap();
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
        let tmp_dir = TempDir::new("test_enrollments")?;
        let db = Database::new(&tmp_dir)?;
        let mut writer = db.write()?;
        let exp1 = get_test_experiments()[0].clone();
        let nimbus_id = Uuid::new_v4();
        let aru = Default::default();
        let app_ctx = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };
        assert_eq!(get_enrollments(&db, &writer)?.len(), 0);

        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
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
        let tmp_dir = TempDir::new("test_updates")?;
        let db = Database::new(&tmp_dir)?;
        let mut writer = db.write()?;
        let nimbus_id = Uuid::new_v4();
        let aru = Default::default();
        let app_ctx = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };
        assert_eq!(get_enrollments(&db, &writer)?.len(), 0);
        let exps = get_test_experiments();

        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
        let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

        let enrollments = get_enrollments(&db, &writer)?;
        assert_eq!(enrollments.len(), 2);
        assert_eq!(events.len(), 2);

        // pretend we just updated from the server and one of the 2 is missing.
        let exps = &[exps[1].clone()];
        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
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
        let tmp_dir = TempDir::new("test_global_opt_out")?;
        let db = Database::new(&tmp_dir)?;
        let mut writer = db.write()?;
        let nimbus_id = Uuid::new_v4();
        let app_ctx = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };
        let aru = Default::default();
        assert_eq!(get_enrollments(&db, &writer)?.len(), 0);
        let exps = get_test_experiments();

        // User has opted out of new experiments.
        set_global_user_participation(&db, &mut writer, false)?;

        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
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

        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
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

        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
        let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

        let enrollments = get_enrollments(&db, &writer)?;
        assert_eq!(enrollments.len(), 0);
        assert_eq!(events.len(), 2);
        // We should see 2 experiment enrolments, this time they're both opt outs
        assert_eq!(get_experiment_enrollments(&db, &writer)?.len(), 2);
        let disqualified_enrollments: Vec<ExperimentEnrollment> =
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
                .collect();
        assert_eq!(disqualified_enrollments.len(), 2);

        // Opting in again and updating SHOULD NOT enroll us again (we've been disqualified).
        set_global_user_participation(&db, &mut writer, true)?;

        let evolver = EnrollmentsEvolver::new(&nimbus_id, &aru, &app_ctx);
        let events = evolver.evolve_enrollments_in_db(&db, &mut writer, &exps)?;

        let enrollments = get_enrollments(&db, &writer)?;
        assert_eq!(enrollments.len(), 0);
        assert!(events.is_empty());
        let disqualified_enrollments: Vec<ExperimentEnrollment> =
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
                .collect();
        assert_eq!(disqualified_enrollments.len(), 2);

        writer.commit()?;
        Ok(())
    }

    #[test]
    fn test_telemetry_reset() -> Result<()> {
        let _ = env_logger::try_init();
        let tmp_dir = TempDir::new("test_telemetry_reset")?;
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
                status: EnrollmentStatus::new_enrolled(
                    EnrolledReason::Qualified,
                    &mock_exp1_branch,
                ),
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
            && ! Uuid::parse_str(&enrollment_id)?.is_nil()
        ));

        Ok(())
    }
}

#[cfg(test)]
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
mod test_schema_bw_compat {
    use super::*;
    use serde_json::json;

    #[test]
    // This was the `ExperimentEnrollment` object schema as it initially shipped to Fenix Nightly.
    // It was missing some fields that have since been added.
    fn test_experiment_enrollment_schema_initial_release() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        let enroll: ExperimentEnrollment = serde_json::from_value(json!({
            "slug": "test",
            "status": {"Enrolled": {
                "enrollment_id": "b6d6f532-e219-4b5a-8ddf-66700dd47d68",
                "reason": "Qualified",
                "branch": "hello",
            }}
        }))
        .unwrap();
        assert!(matches!(enroll.status, EnrollmentStatus::Enrolled { .. }));
    }

    // In #96 we added a `feature_id` field to the ExperimentEnrollment schema.
    // This tests the data as it was after that change.
    #[test]
    fn test_experiment_schema_with_feature_ids() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        let enroll: ExperimentEnrollment = serde_json::from_value(json!({
            "slug": "secure-gold",
            "status": {"Enrolled": {
                "enrollment_id": "b6d6f532-e219-4b5a-8ddf-66700dd47d68",
                "reason": "Qualified",
                "branch": "hello",
                "feature_id": "some_control"
            }}
        }))
        .unwrap();
        assert!(matches!(enroll.status, EnrollmentStatus::Enrolled { .. }));
    }

    // In SDK-260 we added a FeatureConflict variant to the NotEnrolledReason
    // schema.
    #[test]
    fn test_not_enrolled_reason_schema_with_feature_conflict() {
        // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
        let non_enrollment: ExperimentEnrollment = serde_json::from_value(json!({
            "slug": "secure-gold",
            "status": {"NotEnrolled": {
                "reason": "FeatureConflict",
            }}
        }))
        .unwrap();
        assert!(
            matches!(non_enrollment.status, EnrollmentStatus::NotEnrolled{ ref reason, ..} if reason == &NotEnrolledReason::FeatureConflict)
        );
    }
}
