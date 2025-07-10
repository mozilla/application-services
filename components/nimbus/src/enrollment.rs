// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
#[cfg(feature = "stateful")]
use crate::stateful::gecko_prefs::PrefUnenrollReason;
use crate::{
    defaults::Defaults,
    error::{debug, warn, NimbusError, Result},
    evaluator::evaluate_enrollment,
    json, AvailableRandomizationUnits, Experiment, FeatureConfig, NimbusTargetingHelper,
    SLUG_REPLACEMENT_PATTERN,
};
use serde_derive::*;
use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter, Result as FmtResult},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(crate) const PREVIOUS_ENROLLMENTS_GC_TIME: Duration = Duration::from_secs(365 * 24 * 3600);

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

impl Display for EnrolledReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(
            match self {
                EnrolledReason::Qualified => "Qualified",
                EnrolledReason::OptIn => "OptIn",
            },
            f,
        )
    }
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

impl Display for NotEnrolledReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(
            match self {
                NotEnrolledReason::OptOut => "OptOut",
                NotEnrolledReason::NotSelected => "NotSelected",
                NotEnrolledReason::NotTargeted => "NotTargeted",
                NotEnrolledReason::EnrollmentsPaused => "EnrollmentsPaused",
                NotEnrolledReason::FeatureConflict => "FeatureConflict",
            },
            f,
        )
    }
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
    /// The bucketing has changed for an experiment.
    NotSelected,
    /// A pref used in the experiment was set by the user.
    #[cfg(feature = "stateful")]
    PrefUnenrollReason { reason: PrefUnenrollReason },
}

impl Display for DisqualifiedReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(
            match self {
                DisqualifiedReason::Error => "Error",
                DisqualifiedReason::OptOut => "OptOut",
                DisqualifiedReason::NotSelected => "NotSelected",
                DisqualifiedReason::NotTargeted => "NotTargeted",
                #[cfg(feature = "stateful")]
                DisqualifiedReason::PrefUnenrollReason { reason } => match reason {
                    PrefUnenrollReason::Changed => "PrefChanged",
                    PrefUnenrollReason::FailedToSet => "PrefFailedToSet",
                },
            },
            f,
        )
    }
}

// Every experiment has an ExperimentEnrollment, even when we aren't enrolled.

// ⚠️ Attention : Changes to this type should be accompanied by a new test  ⚠️
// ⚠️ in `mod test_schema_bw_compat` below, and may require a DB migration. ⚠️
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ExperimentEnrollment {
    pub slug: String,
    pub status: EnrollmentStatus,
}

impl ExperimentEnrollment {
    /// Evaluate an experiment enrollment for an experiment
    /// we are seeing for the first time.
    fn from_new_experiment(
        is_user_participating: bool,
        available_randomization_units: &AvailableRandomizationUnits,
        experiment: &Experiment,
        targeting_helper: &NimbusTargetingHelper,
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
            let enrollment =
                evaluate_enrollment(available_randomization_units, experiment, targeting_helper)?;
            debug!(
                "Experiment '{}' is new - enrollment status is {:?}",
                &enrollment.slug, &enrollment
            );
            if matches!(enrollment.status, EnrollmentStatus::Enrolled { .. }) {
                out_enrollment_events.push(enrollment.get_change_event())
            }
            enrollment
        })
    }

    /// Force enroll ourselves in an experiment.
    #[cfg_attr(not(feature = "stateful"), allow(unused))]
    pub(crate) fn from_explicit_opt_in(
        experiment: &Experiment,
        branch_slug: &str,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> Result<Self> {
        if !experiment.has_branch(branch_slug) {
            out_enrollment_events.push(EnrollmentChangeEvent {
                experiment_slug: experiment.slug.to_string(),
                branch_slug: branch_slug.to_string(),
                reason: Some("does-not-exist".to_string()),
                change: EnrollmentChangeEventType::EnrollFailed,
            });

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
    #[allow(clippy::too_many_arguments)]
    fn on_experiment_updated(
        &self,
        is_user_participating: bool,
        available_randomization_units: &AvailableRandomizationUnits,
        updated_experiment: &Experiment,
        targeting_helper: &NimbusTargetingHelper,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> Result<Self> {
        Ok(match &self.status {
            EnrollmentStatus::NotEnrolled { .. } | EnrollmentStatus::Error { .. } => {
                if !is_user_participating || updated_experiment.is_enrollment_paused {
                    self.clone()
                } else {
                    let updated_enrollment = evaluate_enrollment(
                        available_randomization_units,
                        updated_experiment,
                        targeting_helper,
                    )?;
                    debug!(
                        "Experiment '{}' with enrollment {:?} is now {:?}",
                        &self.slug, &self, updated_enrollment
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
                    debug!(
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
                        available_randomization_units,
                        updated_experiment,
                        targeting_helper,
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
                            debug!("Existing experiment enrollment '{}' is now disqualified (targeting change)", &self.slug);
                            let updated_enrollment =
                                self.disqualify_from_enrolled(DisqualifiedReason::NotTargeted);
                            out_enrollment_events.push(updated_enrollment.get_change_event());
                            updated_enrollment
                        }
                        EnrollmentStatus::NotEnrolled {
                            reason: NotEnrolledReason::NotSelected,
                        } => {
                            // In the case of a rollout being scaled back, we should be disqualified with NotSelected.
                            //
                            let updated_enrollment =
                                self.disqualify_from_enrolled(DisqualifiedReason::NotSelected);
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
                ref branch, reason, ..
            } => {
                if !is_user_participating {
                    debug!(
                        "Disqualified experiment enrollment '{}' has been reset to not-enrolled (global opt-out)",
                        &self.slug
                    );
                    Self {
                        slug: self.slug.clone(),
                        status: EnrollmentStatus::Disqualified {
                            reason: DisqualifiedReason::OptOut,
                            branch: branch.clone(),
                        },
                    }
                } else if updated_experiment.is_rollout
                    && matches!(
                        reason,
                        DisqualifiedReason::NotSelected | DisqualifiedReason::NotTargeted,
                    )
                {
                    let evaluated_enrollment = evaluate_enrollment(
                        available_randomization_units,
                        updated_experiment,
                        targeting_helper,
                    )?;
                    match evaluated_enrollment.status {
                        EnrollmentStatus::Enrolled { .. } => evaluated_enrollment,
                        _ => self.clone(),
                    }
                } else {
                    self.clone()
                }
            }
            EnrollmentStatus::WasEnrolled { .. } => self.clone(),
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
        debug!(
            "Experiment '{}' vanished while we had enrollment status of {:?}",
            self.slug, self
        );
        let branch = match self.status {
            EnrollmentStatus::Enrolled { ref branch, .. }
            | EnrollmentStatus::Disqualified { ref branch, .. } => branch,
            EnrollmentStatus::NotEnrolled { .. }
            | EnrollmentStatus::WasEnrolled { .. }
            | EnrollmentStatus::Error { .. } => return None, // We were never enrolled anyway, simply delete the enrollment record from the DB.
        };
        let enrollment = Self {
            slug: self.slug.clone(),
            status: EnrollmentStatus::WasEnrolled {
                branch: branch.to_owned(),
                experiment_ended_at: now_secs(),
            },
        };
        out_enrollment_events.push(enrollment.get_change_event());
        Some(enrollment)
    }

    /// Force unenroll ourselves from an experiment.
    #[allow(clippy::unnecessary_wraps)]
    #[cfg_attr(not(feature = "stateful"), allow(unused))]
    pub(crate) fn on_explicit_opt_out(
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

    #[cfg(feature = "stateful")]
    pub(crate) fn on_pref_unenroll(
        &self,
        pref_unenroll_reason: PrefUnenrollReason,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>,
    ) -> ExperimentEnrollment {
        match self.status {
            EnrollmentStatus::Enrolled { .. } => {
                let enrollment =
                    self.disqualify_from_enrolled(DisqualifiedReason::PrefUnenrollReason {
                        reason: pref_unenroll_reason,
                    });
                out_enrollment_events.push(enrollment.get_change_event());
                enrollment
            }
            _ => self.clone(),
        }
    }

    /// Reset identifiers in response to application-level telemetry reset.
    ///
    /// We move any enrolled experiments to the "disqualified" state, since their further
    /// partipation would submit partial data that could skew analysis.
    ///
    #[cfg_attr(not(feature = "stateful"), allow(unused))]
    pub fn reset_telemetry_identifiers(
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
            status: updated.status.clone(),
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
        debug!("Garbage collecting enrollment '{}'", self.slug);
        None
    }

    // Create a telemetry event describing the transition
    // to the current enrollment state.
    fn get_change_event(&self) -> EnrollmentChangeEvent {
        match &self.status {
            EnrollmentStatus::Enrolled { branch, .. } => EnrollmentChangeEvent::new(
                &self.slug,
                branch,
                None,
                EnrollmentChangeEventType::Enrollment,
            ),
            EnrollmentStatus::WasEnrolled { branch, .. } => EnrollmentChangeEvent::new(
                &self.slug,
                branch,
                None,
                EnrollmentChangeEventType::Unenrollment,
            ),
            EnrollmentStatus::Disqualified { branch, reason, .. } => EnrollmentChangeEvent::new(
                &self.slug,
                branch,
                match reason {
                    DisqualifiedReason::NotSelected => Some("bucketing"),
                    DisqualifiedReason::NotTargeted => Some("targeting"),
                    DisqualifiedReason::OptOut => Some("optout"),
                    DisqualifiedReason::Error => Some("error"),
                    #[cfg(feature = "stateful")]
                    DisqualifiedReason::PrefUnenrollReason { reason } => match reason {
                        PrefUnenrollReason::Changed => Some("pref_changed"),
                        PrefUnenrollReason::FailedToSet => Some("pref_failed_to_set"),
                    },
                },
                EnrollmentChangeEventType::Disqualification,
            ),
            EnrollmentStatus::NotEnrolled { .. } | EnrollmentStatus::Error { .. } => {
                unreachable!()
            }
        }
    }

    /// If the current state is `Enrolled`, move to `Disqualified` with the given reason.
    fn disqualify_from_enrolled(&self, reason: DisqualifiedReason) -> Self {
        match self.status {
            EnrollmentStatus::Enrolled { ref branch, .. } => ExperimentEnrollment {
                status: EnrollmentStatus::Disqualified {
                    reason,
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
        reason: EnrolledReason,
        branch: String,
    },
    NotEnrolled {
        reason: NotEnrolledReason,
    },
    Disqualified {
        reason: DisqualifiedReason,
        branch: String,
    },
    WasEnrolled {
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
    pub fn name(&self) -> String {
        match self {
            EnrollmentStatus::Enrolled { .. } => "Enrolled",
            EnrollmentStatus::NotEnrolled { .. } => "NotEnrolled",
            EnrollmentStatus::Disqualified { .. } => "Disqualified",
            EnrollmentStatus::WasEnrolled { .. } => "WasEnrolled",
            EnrollmentStatus::Error { .. } => "Error",
        }
        .into()
    }
}

impl EnrollmentStatus {
    // Note that for now, we only support a single feature_id per experiment,
    // so this code is expected to shift once we start supporting multiple.
    pub fn new_enrolled(reason: EnrolledReason, branch: &str) -> Self {
        EnrollmentStatus::Enrolled {
            reason,
            branch: branch.to_owned(),
        }
    }

    // This is used in examples, but not in the main dylib, and
    // triggers a dead code warning when building with `--release`.
    pub fn is_enrolled(&self) -> bool {
        matches!(self, EnrollmentStatus::Enrolled { .. })
    }
}

pub(crate) trait ExperimentMetadata {
    fn get_slug(&self) -> String;

    fn is_rollout(&self) -> bool;
}

pub(crate) struct EnrollmentsEvolver<'a> {
    available_randomization_units: &'a AvailableRandomizationUnits,
    targeting_helper: &'a mut NimbusTargetingHelper,
    coenrolling_feature_ids: &'a HashSet<&'a str>,
}

impl<'a> EnrollmentsEvolver<'a> {
    pub(crate) fn new(
        available_randomization_units: &'a AvailableRandomizationUnits,
        targeting_helper: &'a mut NimbusTargetingHelper,
        coenrolling_feature_ids: &'a HashSet<&str>,
    ) -> Self {
        Self {
            available_randomization_units,
            targeting_helper,
            coenrolling_feature_ids,
        }
    }

    pub(crate) fn evolve_enrollments<E>(
        &mut self,
        is_user_participating: bool,
        prev_experiments: &[E],
        next_experiments: &[Experiment],
        prev_enrollments: &[ExperimentEnrollment],
    ) -> Result<(Vec<ExperimentEnrollment>, Vec<EnrollmentChangeEvent>)>
    where
        E: ExperimentMetadata + Clone,
    {
        let mut enrollments: Vec<ExperimentEnrollment> = Default::default();
        let mut events: Vec<EnrollmentChangeEvent> = Default::default();

        // Do rollouts first.
        // At the moment, we only allow one rollout per feature, so we can re-use the same machinery as experiments
        let (prev_rollouts, ro_enrollments) = filter_experiments_and_enrollments(
            prev_experiments,
            prev_enrollments,
            ExperimentMetadata::is_rollout,
        );
        let next_rollouts = filter_experiments(next_experiments, ExperimentMetadata::is_rollout);

        let (next_ro_enrollments, ro_events) = self.evolve_enrollment_recipes(
            is_user_participating,
            &prev_rollouts,
            &next_rollouts,
            &ro_enrollments,
        )?;

        enrollments.extend(next_ro_enrollments);
        events.extend(ro_events);

        let ro_slugs: HashSet<String> = ro_enrollments.iter().map(|e| e.slug.clone()).collect();

        // Now we do the experiments.
        // We need to mop up all the enrollments that aren't rollouts (not just belonging to experiments that aren't rollouts)
        // because some of them don't belong to any experiments recipes, and evolve_enrollment_recipes will handle the error
        // states for us.
        let prev_experiments = filter_experiments(prev_experiments, |exp| !exp.is_rollout());
        let next_experiments = filter_experiments(next_experiments, |exp| !exp.is_rollout());
        let prev_enrollments: Vec<ExperimentEnrollment> = prev_enrollments
            .iter()
            .filter(|e| !ro_slugs.contains(&e.slug))
            .map(|e| e.to_owned())
            .collect();

        let (next_exp_enrollments, exp_events) = self.evolve_enrollment_recipes(
            is_user_participating,
            &prev_experiments,
            &next_experiments,
            &prev_enrollments,
        )?;

        enrollments.extend(next_exp_enrollments);
        events.extend(exp_events);

        Ok((enrollments, events))
    }

    /// Evolve and calculate the new set of enrollments, using the
    /// previous and current state of experiments and current enrollments.
    pub(crate) fn evolve_enrollment_recipes<E>(
        &mut self,
        is_user_participating: bool,
        prev_experiments: &[E],
        next_experiments: &[Experiment],
        prev_enrollments: &[ExperimentEnrollment],
    ) -> Result<(Vec<ExperimentEnrollment>, Vec<EnrollmentChangeEvent>)>
    where
        E: ExperimentMetadata + Clone,
    {
        let mut enrollment_events = vec![];
        let prev_experiments_map = map_experiments(prev_experiments);
        let next_experiments_map = map_experiments(next_experiments);
        let prev_enrollments_map = map_enrollments(prev_enrollments);

        // Step 1. Build an initial active_features to keep track of
        // the features that are being experimented upon.
        let mut enrolled_features = HashMap::with_capacity(next_experiments.len());
        let mut coenrolling_features = HashMap::with_capacity(next_experiments.len());

        let mut next_enrollments = Vec::with_capacity(next_experiments.len());

        // Step 2.
        // Evolve the experiments with previous enrollments first (except for
        // those that already have a feature conflict).  While we're doing so,
        // start building up active_features, the map of feature_ids under
        // experiment to EnrolledFeatureConfigs, and next_enrollments.

        for prev_enrollment in prev_enrollments {
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
                prev_experiments_map.get(slug).copied(),
                next_experiments_map.get(slug).copied(),
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
                    warn!("{} in evolve_enrollment (with prev_enrollment) returned None; (slug: {}, prev_enrollment: {:?}); ", e, slug, prev_enrollment);
                    None
                }
            };

            #[cfg(feature = "stateful")]
            if let Some(ref enrollment) = next_enrollment.clone() {
                if self.targeting_helper.update_enrollment(enrollment) {
                    debug!("Enrollment updated for {}", enrollment.slug);
                } else {
                    debug!("Enrollment unchanged for {}", enrollment.slug);
                }
            }

            self.reserve_enrolled_features(
                next_enrollment,
                &next_experiments_map,
                &mut enrolled_features,
                &mut coenrolling_features,
                &mut next_enrollments,
            );
        }

        // Step 3. Evolve the remaining enrollments with the previous and
        // next data.
        let next_experiments = sort_experiments_by_published_date(next_experiments);
        for next_experiment in next_experiments {
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

                    enrollment_events.push(EnrollmentChangeEvent {
                        experiment_slug: slug.clone(),
                        branch_slug: "N/A".to_string(),
                        reason: Some("feature-conflict".to_string()),
                        change: EnrollmentChangeEventType::EnrollFailed,
                    })
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
            let prev_enrollment = prev_enrollments_map.get(slug).copied();

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
                    prev_experiments_map.get(slug).copied(),
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
                        warn!("{} in evolve_enrollment (with no feature conflict) returned None; (slug: {}, prev_enrollment: {:?}); ", e, slug, prev_enrollment);
                        None
                    }
                };

                #[cfg(feature = "stateful")]
                if let Some(ref enrollment) = next_enrollment.clone() {
                    if self.targeting_helper.update_enrollment(enrollment) {
                        debug!("Enrollment updated for {}", enrollment.slug);
                    } else {
                        debug!("Enrollment unchanged for {}", enrollment.slug);
                    }
                }

                self.reserve_enrolled_features(
                    next_enrollment,
                    &next_experiments_map,
                    &mut enrolled_features,
                    &mut coenrolling_features,
                    &mut next_enrollments,
                );
            }
        }

        enrolled_features.extend(coenrolling_features);

        // Check that we generate the enrolled feature map from the new
        // enrollments and new experiments.  Perhaps this should just be an
        // assert.
        let updated_enrolled_features = map_features(
            &next_enrollments,
            &next_experiments_map,
            self.coenrolling_feature_ids,
        );
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
        coenrolling_features: &mut HashMap<String, EnrolledFeatureConfig>,
        enrollments: &mut Vec<ExperimentEnrollment>,
    ) {
        if let Some(enrollment) = latest_enrollment {
            // Now we have an enrollment object!
            // If it's an enrolled enrollment, then get the FeatureConfigs
            // from the experiment and store them in the enrolled_features or coenrolling_features maps.
            for enrolled_feature in get_enrolled_feature_configs(&enrollment, experiments) {
                populate_feature_maps(
                    enrolled_feature,
                    self.coenrolling_feature_ids,
                    enrolled_features,
                    coenrolling_features,
                );
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
    pub(crate) fn evolve_enrollment<E>(
        &mut self,
        is_user_participating: bool,
        prev_experiment: Option<&E>,
        next_experiment: Option<&Experiment>,
        prev_enrollment: Option<&ExperimentEnrollment>,
        out_enrollment_events: &mut Vec<EnrollmentChangeEvent>, // out param containing the events we'd like to emit to glean.
    ) -> Result<Option<ExperimentEnrollment>>
    where
        E: ExperimentMetadata + Clone,
    {
        let is_already_enrolled = if let Some(enrollment) = prev_enrollment {
            enrollment.status.is_enrolled()
        } else {
            false
        };

        // XXX This is not pretty, however, we need to re-write the way sticky targeting strings are generated in
        // experimenter. Once https://github.com/mozilla/experimenter/issues/8661 is fixed, we can remove the calculation
        // for `is_already_enrolled` above, the `put` call here and the `put` method declaration, and replace it with
        // let th = self.targeting_helper;
        let targeting_helper = self
            .targeting_helper
            .put("is_already_enrolled", is_already_enrolled);

        Ok(match (prev_experiment, next_experiment, prev_enrollment) {
            // New experiment.
            (None, Some(experiment), None) => Some(ExperimentEnrollment::from_new_experiment(
                is_user_participating,
                self.available_randomization_units,
                experiment,
                &targeting_helper,
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
                    self.available_randomization_units,
                    experiment,
                    &targeting_helper,
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

fn map_experiments<E>(experiments: &[E]) -> HashMap<String, &E>
where
    E: ExperimentMetadata + Clone,
{
    let mut map_experiments = HashMap::with_capacity(experiments.len());
    for e in experiments {
        map_experiments.insert(e.get_slug(), e);
    }
    map_experiments
}

pub fn map_enrollments(
    enrollments: &[ExperimentEnrollment],
) -> HashMap<String, &ExperimentEnrollment> {
    let mut map_enrollments = HashMap::with_capacity(enrollments.len());
    for e in enrollments {
        map_enrollments.insert(e.slug.clone(), e);
    }
    map_enrollments
}

pub(crate) fn filter_experiments_and_enrollments<E>(
    experiments: &[E],
    enrollments: &[ExperimentEnrollment],
    filter_fn: fn(&E) -> bool,
) -> (Vec<E>, Vec<ExperimentEnrollment>)
where
    E: ExperimentMetadata + Clone,
{
    let experiments: Vec<E> = filter_experiments(experiments, filter_fn);

    let slugs: HashSet<String> = experiments.iter().map(|e| e.get_slug()).collect();

    let enrollments: Vec<ExperimentEnrollment> = enrollments
        .iter()
        .filter(|e| slugs.contains(&e.slug))
        .map(|e| e.to_owned())
        .collect();

    (experiments, enrollments)
}

fn filter_experiments<E>(experiments: &[E], filter_fn: fn(&E) -> bool) -> Vec<E>
where
    E: ExperimentMetadata + Clone,
{
    experiments
        .iter()
        .filter(|e| filter_fn(e))
        .cloned()
        .collect()
}

pub(crate) fn sort_experiments_by_published_date(experiments: &[Experiment]) -> Vec<&Experiment> {
    let mut experiments: Vec<_> = experiments.iter().collect();
    experiments.sort_by(|a, b| a.published_date.cmp(&b.published_date));
    experiments
}

/// Take a list of enrollments and a map of experiments, and generate mapping of `feature_id` to
/// `EnrolledFeatureConfig` structs.
fn map_features(
    enrollments: &[ExperimentEnrollment],
    experiments: &HashMap<String, &Experiment>,
    coenrolling_ids: &HashSet<&str>,
) -> HashMap<String, EnrolledFeatureConfig> {
    let mut colliding_features = HashMap::with_capacity(enrollments.len());
    let mut coenrolling_features = HashMap::with_capacity(enrollments.len());
    for enrolled_feature_config in enrollments
        .iter()
        .flat_map(|e| get_enrolled_feature_configs(e, experiments))
    {
        populate_feature_maps(
            enrolled_feature_config,
            coenrolling_ids,
            &mut colliding_features,
            &mut coenrolling_features,
        );
    }
    colliding_features.extend(coenrolling_features.drain());

    colliding_features
}

pub fn map_features_by_feature_id(
    enrollments: &[ExperimentEnrollment],
    experiments: &[Experiment],
    coenrolling_ids: &HashSet<&str>,
) -> HashMap<String, EnrolledFeatureConfig> {
    let (rollouts, ro_enrollments) = filter_experiments_and_enrollments(
        experiments,
        enrollments,
        ExperimentMetadata::is_rollout,
    );
    let (experiments, exp_enrollments) =
        filter_experiments_and_enrollments(experiments, enrollments, |exp| !exp.is_rollout());

    let features_under_rollout = map_features(
        &ro_enrollments,
        &map_experiments(&rollouts),
        coenrolling_ids,
    );
    let features_under_experiment = map_features(
        &exp_enrollments,
        &map_experiments(&experiments),
        coenrolling_ids,
    );

    features_under_experiment
        .defaults(&features_under_rollout)
        .unwrap()
}

pub(crate) fn populate_feature_maps(
    enrolled_feature: EnrolledFeatureConfig,
    coenrolling_feature_ids: &HashSet<&str>,
    colliding_features: &mut HashMap<String, EnrolledFeatureConfig>,
    coenrolling_features: &mut HashMap<String, EnrolledFeatureConfig>,
) {
    let feature_id = &enrolled_feature.feature_id;
    if !coenrolling_feature_ids.contains(feature_id.as_str()) {
        // If we're not allowing co-enrollment for this feature, then add it to enrolled_features.
        // We'll use this map to prevent collisions.
        colliding_features.insert(feature_id.clone(), enrolled_feature);
    } else if let Some(existing) = coenrolling_features.get(feature_id) {
        // Otherwise, we'll add to the coenrolling_features map.
        // In this branch, we've enrolled in one experiment already before this one.
        // We take care to merge this one with the existing one.
        let merged = enrolled_feature
            .defaults(existing)
            .expect("A feature config hasn't been able to merge; this is a bug in Nimbus");

        // We change the branch to None, so we don't send exposure events from this feature.
        // This is the subject of the ADR for https://mozilla-hub.atlassian.net/browse/EXP-3630.
        let merged = EnrolledFeatureConfig {
            // We make up the slug by appending. This is only for debugging reasons.
            slug: format!("{}+{}", &existing.slug, &enrolled_feature.slug),
            branch: None,
            ..merged
        };
        coenrolling_features.insert(feature_id.clone(), merged);
    } else {
        // In this branch, this is the first time we've added this feature to the coenrolling_features map.
        coenrolling_features.insert(feature_id.clone(), enrolled_feature);
    }
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
    let mut branch_features = match &experiment.get_branch(branch_slug) {
        Some(branch) => branch.get_feature_configs(),
        _ => Default::default(),
    };

    branch_features.iter_mut().for_each(|f| {
        json::replace_str_in_map(&mut f.value, SLUG_REPLACEMENT_PATTERN, experiment_slug);
    });

    let branch_feature_ids = &branch_features
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
            branch: if !experiment.is_rollout() {
                Some(branch_slug.clone())
            } else {
                None
            },
            feature_id: f.feature_id.clone(),
        })
        .collect()
}

/// Small transitory struct to contain all the information needed to configure a feature with the Feature API.
/// By design, we don't want to store it on the disk. Instead we calculate it from experiments
/// and enrollments.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnrolledFeatureConfig {
    pub feature: FeatureConfig,
    pub slug: String,
    pub branch: Option<String>,
    pub feature_id: String,
}

impl Defaults for EnrolledFeatureConfig {
    fn defaults(&self, fallback: &Self) -> Result<Self> {
        if self.feature_id != fallback.feature_id {
            // This is unlikely to happen, but if it does it's a bug in Nimbus
            Err(NimbusError::InternalError(
                "Cannot merge enrolled feature configs from different features",
            ))
        } else {
            Ok(Self {
                slug: self.slug.to_owned(),
                feature_id: self.feature_id.to_owned(),
                // Merge the actual feature config.
                feature: self.feature.defaults(&fallback.feature)?,
                // If this is an experiment, then this will be Some(_).
                // The feature is involved in zero or one experiments, and 0 or more rollouts.
                // So we can clone this Option safely.
                branch: self.branch.to_owned(),
            })
        }
    }
}

impl ExperimentMetadata for EnrolledFeatureConfig {
    fn get_slug(&self) -> String {
        self.slug.clone()
    }

    fn is_rollout(&self) -> bool {
        self.branch.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrolledFeature {
    pub slug: String,
    pub branch: Option<String>,
    pub feature_id: String,
}

impl From<&EnrolledFeatureConfig> for EnrolledFeature {
    fn from(value: &EnrolledFeatureConfig) -> Self {
        Self {
            slug: value.slug.clone(),
            branch: value.branch.clone(),
            feature_id: value.feature_id.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnrollmentChangeEvent {
    pub experiment_slug: String,
    pub branch_slug: String,
    pub reason: Option<String>,
    pub change: EnrollmentChangeEventType,
}

impl EnrollmentChangeEvent {
    pub(crate) fn new(
        slug: &str,
        branch: &str,
        reason: Option<&str>,
        change: EnrollmentChangeEventType,
    ) -> Self {
        Self {
            experiment_slug: slug.to_owned(),
            branch_slug: branch.to_owned(),
            reason: reason.map(|s| s.to_owned()),
            change,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum EnrollmentChangeEventType {
    Enrollment,
    EnrollFailed,
    Disqualification,
    Unenrollment,
    #[cfg_attr(not(feature = "stateful"), allow(unused))]
    UnenrollFailed,
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current date before Unix Epoch.")
        .as_secs()
}
