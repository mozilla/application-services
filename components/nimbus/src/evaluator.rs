/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde_derive::*;
use serde_json::Value;

use crate::enrollment::{
    EnrolledReason, EnrollmentStatus, ExperimentEnrollment, NotEnrolledReason,
};
use crate::error::{NimbusError, Result, debug, info};
use crate::sampling;
#[cfg(feature = "stateful")]
pub use crate::stateful::evaluator::*;
#[cfg(not(feature = "stateful"))]
pub use crate::stateless::evaluator::*;
use crate::{AvailableRandomizationUnits, Branch, Experiment, NimbusTargetingHelper};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Bucket {}

impl Bucket {
    #[allow(unused)]
    pub fn new() -> Self {
        unimplemented!();
    }
}

fn prefer_none_to_empty(s: Option<&str>) -> Option<String> {
    let s = s?;
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

pub fn split_locale(locale: String) -> (Option<String>, Option<String>) {
    if locale.contains('-') {
        let mut parts = locale.split('-');
        (
            prefer_none_to_empty(parts.next()),
            prefer_none_to_empty(parts.next()),
        )
    } else {
        (Some(locale), None)
    }
}

/// Determine the enrolment status for an experiment.
///
/// # Errors
///
/// The function can return an error when branch selection fails due to an
/// invalid bucketing configuration.
pub fn evaluate_enrollment(
    available_randomization_units: &AvailableRandomizationUnits,
    experiment: &Experiment,
    targeting_helper: &NimbusTargetingHelper,
) -> Result<ExperimentEnrollment> {
    let status = match can_enroll(available_randomization_units, targeting_helper, experiment) {
        CanEnrollResult::Unavailable { reason } => EnrollmentStatus::NotEnrolled { reason },
        CanEnrollResult::NotTargeted => EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted,
        },
        CanEnrollResult::NotSelected => EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotSelected,
        },
        CanEnrollResult::TargetingError { reason } => EnrollmentStatus::Error { reason },
        CanEnrollResult::NoRandomizationUnit => {
            info!(
                "Could not find a suitable randomization unit for {}. Skipping experiment.",
                experiment.slug,
            );
            EnrollmentStatus::Error {
                reason: "No randomization unit".into(),
            }
        }

        CanEnrollResult::Enrollable { randomization_id } => EnrollmentStatus::new_enrolled(
            EnrolledReason::Qualified,
            &choose_branch(&experiment.slug, &experiment.branches, randomization_id)?.slug,
        ),
    };

    Ok(ExperimentEnrollment {
        slug: experiment.slug.clone(),
        status,
    })
}

/// Whether or not an experiment can be enrolled.
pub enum CanEnrollResult<'aru> {
    /// The experiment is enrollable.
    Enrollable {
        /// The randomization ID that should be used for branch selection.
        randomization_id: &'aru str,
    },

    /// The experiment is not available for a reason outlined in [`NotEnrolledReason`]
    Unavailable {
        /// The reason the enrollment is not available.
        reason: NotEnrolledReason,
    },

    /// The experiment is not enrollable due to a targeting error.
    TargetingError {
        /// The stringified error.
        reason: String,
    },

    /// The experiment is not enrollable because targeting expression evaluated
    /// to false.
    NotTargeted,

    /// The experiment is not enrollable because randomization ID did not fall
    /// into a selected bucket.
    NotSelected,

    /// The experiment is not enrollable because it requires a randomization
    /// unit that is not available.
    NoRandomizationUnit,
}

/// Determine whether or not it is possible to enroll in the given experiment.
pub fn can_enroll<'aru>(
    available_randomization_units: &'aru AvailableRandomizationUnits,
    targeting_helper: &NimbusTargetingHelper,
    experiment: &Experiment,
) -> CanEnrollResult<'aru> {
    if let ExperimentAvailable::Unavailable { reason } =
        is_experiment_available(targeting_helper, experiment, true)
    {
        return CanEnrollResult::Unavailable { reason };
    }

    if let Some(targeting_expression) = &experiment.targeting {
        match targeting_helper.eval_jexl(targeting_expression) {
            Err(e) => {
                return CanEnrollResult::TargetingError {
                    reason: e.to_string(),
                };
            }
            Ok(false) => return CanEnrollResult::NotTargeted,
            Ok(true) => {}
        };
    }

    let Some(randomization_id) =
        available_randomization_units.get_value(&experiment.bucket_config.randomization_unit)
    else {
        return CanEnrollResult::NoRandomizationUnit;
    };

    let Ok(is_sampled) = sampling::bucket_sample(
        [randomization_id, &experiment.bucket_config.namespace],
        experiment.bucket_config.start,
        experiment.bucket_config.count,
        experiment.bucket_config.total,
    ) else {
        return CanEnrollResult::NoRandomizationUnit;
    };

    if is_sampled {
        CanEnrollResult::Enrollable { randomization_id }
    } else {
        CanEnrollResult::NotSelected
    }
}

/// Whether or not an experiment is available.
#[derive(Debug, Eq, PartialEq)]
pub enum ExperimentAvailable {
    /// The experiment is available (i.e., it is for this application and channel).
    Available,

    /// The experiment is not available (i.e., it is either not for this
    /// application or not for this channel).
    Unavailable { reason: NotEnrolledReason },
}

/// Check if an experiment is available for this app defined by this `AppContext`.
///
/// # Arguments:
/// - `app_context` The application parameters to use for targeting purposes
/// - `exp` The `Experiment` to evaluate
/// - `is_release` Supports two modes:
///   if `true`, available means available for enrollment: i.e. does the `app_name` and `channel` match.
///   if `false`, available means available for testing: i.e. does only the `app_name` match.
///
/// # Returns:
/// Returns `true` if the experiment matches the targeting
pub fn is_experiment_available(
    th: &NimbusTargetingHelper,
    exp: &Experiment,
    is_release: bool,
) -> ExperimentAvailable {
    // Verify the app_name matches the application being targeted
    // by the experiment.
    match (&exp.app_name, th.context.get("app_name".to_string())) {
        (Some(exp), Some(Value::String(mine))) => {
            if !exp.eq(mine) {
                return ExperimentAvailable::Unavailable {
                    reason: NotEnrolledReason::DifferentAppName,
                };
            }
        }
        (_, _) => debug!("Experiment missing app_name, skipping it as a targeting parameter"),
    }

    if !is_release {
        return ExperimentAvailable::Available;
    }

    // Verify the channel matches the application being targeted
    // by the experiment.  Note, we are intentionally comparing in a case-insensitive way.
    // See https://jira.mozilla.com/browse/SDK-246 for more info.
    match (&exp.channel, th.context.get("channel".to_string())) {
        (Some(exp), Some(Value::String(mine))) => {
            if !exp.to_lowercase().eq(&mine.to_lowercase()) {
                return ExperimentAvailable::Unavailable {
                    reason: NotEnrolledReason::DifferentChannel,
                };
            }
        }
        (_, _) => debug!("Experiment missing channel, skipping it as a targeting parameter"),
    }

    ExperimentAvailable::Available
}

/// Chooses a branch randomly from a set of branches
/// based on the ratios set in the branches
///
/// It is important that the input to the sampling algorithm be:
/// - Unique per-user (no one is bucketed alike)
/// - Unique per-experiment (bucketing differs across multiple experiments)
/// - Differs from the input used for sampling the recipe (otherwise only
///   branches that contain the same buckets as the recipe sampling will
///   receive users)
///
/// # Arguments:
/// - `slug` the slug associated with the experiment
/// - `branches` the branches to pick from
/// - `id` the user id used to pick a branch
///
/// # Returns:
/// Returns the slug for the selected branch
///
/// # Errors:
///
/// An error could occur if something goes wrong while sampling the ratios
pub(crate) fn choose_branch<'a>(
    slug: &str,
    branches: &'a [Branch],
    id: &str,
) -> Result<&'a Branch> {
    // convert from i32 to u32 to work around SDK-175.
    let ratios = branches.iter().map(|b| b.ratio as u32).collect::<Vec<_>>();
    // Note: The "experiment-manager" here comes from
    // https://searchfox.org/mozilla-central/rev/1843375acbbca68127713e402be222350ac99301/toolkit/components/messaging-system/experiments/ExperimentManager.jsm#469
    // TODO: Change it to be something more related to the SDK if it is needed
    let input = format!("{:}-{:}-{:}-branch", "experimentmanager", id, slug);
    let index = sampling::ratio_sample(input, &ratios)?;
    branches.get(index).ok_or(NimbusError::OutOfBoundsError)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_splitting_locale() -> Result<()> {
        assert_eq!(
            split_locale("en-US".to_string()),
            (Some("en".to_string()), Some("US".to_string()))
        );
        assert_eq!(
            split_locale("es".to_string()),
            (Some("es".to_string()), None)
        );

        assert_eq!(
            split_locale("-unknown".to_string()),
            (None, Some("unknown".to_string()))
        );
        Ok(())
    }
}
