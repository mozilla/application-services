/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use crate::defaults::Defaults;
use crate::enrollment::{
    EnrolledReason, EnrollmentStatus, ExperimentEnrollment, NotEnrolledReason,
};
use crate::{
    error::{NimbusError, Result},
    AvailableRandomizationUnits,
};
use crate::{matcher::AppContext, sampling};
use crate::{Branch, Experiment};
use jexl_eval::Evaluator;
use serde_derive::*;
use serde_json::{json, Value};
use uuid::Uuid;
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Bucket {}

impl Bucket {
    #[allow(unused)]
    pub fn new() -> Self {
        unimplemented!();
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TargetingAttributes {
    #[serde(flatten)]
    pub app_context: AppContext,
    pub is_already_enrolled: bool,
    pub days_since_install: Option<i32>,
    pub days_since_update: Option<i32>,
}

/// Determine the enrolment status for an experiment.
///
/// # Arguments:
/// - `nimbus_id` The auto-generated nimbus_id
/// - `available_randomization_units` The app provded available randomization units
/// - `targeting_attributes` The attributes to use when evaluating targeting
/// - `exp` The `Experiment` to evaluate.
///
/// # Returns:
/// An `ExperimentEnrollment` -  you need to inspect the EnrollmentStatus to
/// determine if the user is actually enrolled.
///
/// # Errors:
///
/// The function can return errors in one of the following cases (but not limited to):
///
/// - If the bucket sampling failed (i.e we could not find if the user should or should not be enrolled in the experiment based on the bucketing)
/// - If an error occurs while determining the branch the user should be enrolled in any of the experiments
pub fn evaluate_enrollment(
    nimbus_id: &Uuid,
    available_randomization_units: &AvailableRandomizationUnits,
    targeting_attributes: &TargetingAttributes,
    exp: &Experiment,
) -> Result<ExperimentEnrollment> {
    if !is_experiment_available(&targeting_attributes.app_context, exp, true) {
        return Ok(ExperimentEnrollment {
            slug: exp.slug.clone(),
            status: EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted,
            },
        });
    }

    // Get targeting out of the way - "if let chains" are experimental,
    // otherwise we could improve this.
    if let Some(expr) = &exp.targeting {
        if let Some(status) = targeting(expr, targeting_attributes) {
            return Ok(ExperimentEnrollment {
                slug: exp.slug.clone(),
                status,
            });
        }
    }
    Ok(ExperimentEnrollment {
        slug: exp.slug.clone(),
        status: {
            let bucket_config = exp.bucket_config.clone();
            match available_randomization_units
                .get_value(&nimbus_id.to_string(), &bucket_config.randomization_unit)
            {
                Some(id) => {
                    if sampling::bucket_sample(
                        vec![id.to_owned(), bucket_config.namespace],
                        bucket_config.start,
                        bucket_config.count,
                        bucket_config.total,
                    )? {
                        EnrollmentStatus::new_enrolled(
                            EnrolledReason::Qualified,
                            &choose_branch(&exp.slug, &exp.branches, id)?.clone().slug,
                        )
                    } else {
                        EnrollmentStatus::NotEnrolled {
                            reason: NotEnrolledReason::NotSelected,
                        }
                    }
                }
                None => {
                    // XXX: When we link in glean, it would be nice if we could emit
                    // a failure telemetry event here.
                    log::info!(
                        "Could not find a suitable randomization unit for {}. Skipping experiment.",
                        &exp.slug
                    );
                    EnrollmentStatus::Error {
                        reason: "No randomization unit".into(),
                    }
                }
            }
        },
    })
}

/// Check if an experiment is available for this app defined by this `AppContext`.
///
/// # Arguments:
/// - `app_context` The application parameters to use for targeting purposes
/// - `exp` The `Experiment` to evaluate
/// - `is_release` Supports two modes:
///     if `true`, available means available for enrollment: i.e. does the `app_name` and `channel` match.
///     if `false`, available means available for testing: i.e. does only the `app_name` match.
///
/// # Returns:
/// Returns `true` if the experiment matches the targeting
pub fn is_experiment_available(
    app_context: &AppContext,
    exp: &Experiment,
    is_release: bool,
) -> bool {
    // Verify the app_name matches the application being targeted
    // by the experiment.
    match &exp.app_name {
        Some(app_name) => {
            if !app_name.eq(&app_context.app_name) {
                return false;
            }
        }
        None => log::debug!("Experiment missing app_name, skipping it as a targeting parameter"),
    }

    if !is_release {
        return true;
    }

    // Verify the channel matches the application being targeted
    // by the experiment.  Note, we are intentionally comparing in a case-insensitive way.
    // See https://jira.mozilla.com/browse/SDK-246 for more info.
    match &exp.channel {
        Some(channel) => {
            if !channel
                .to_lowercase()
                .eq(&app_context.channel.to_lowercase())
            {
                return false;
            }
        }
        None => log::debug!("Experiment missing channel, skipping it as a targeting parameter"),
    }
    true
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
    let index = sampling::ratio_sample(&input, &ratios)?;
    branches.get(index).ok_or(NimbusError::OutOfBoundsError)
}

/// Checks if the client is targeted by an experiment
/// This api evaluates the JEXL statement retrieved from the server
/// against the application context provided by the client
///
/// # Arguments
/// - `expression_statement`: The JEXL statement provided by the server
/// - `targeting_attributes`: The client attributes to target against
///
/// If this app can not be targeted, returns an EnrollmentStatus to indicate
/// why. Returns None if we should continue to evaluate the enrollment status.
///
/// In practice, if this returns an EnrollmentStatus, it will be either
/// EnrollmentStatus::NotEnrolled, or EnrollmentStatus::Error in the following
/// cases (But not limited to):
/// - The `expression_statement` is not a valid JEXL statement
/// - The `expression_statement` expects fields that do not exist in the AppContext definition
/// - The result of evaluating the statement against the context is not a boolean
/// - jexl-rs returned an error
pub(crate) fn targeting(
    expression_statement: &str,
    targeting_attributes: &TargetingAttributes,
) -> Option<EnrollmentStatus> {
    match jexl_eval(expression_statement, targeting_attributes, None) {
        Ok(res) => match res {
            true => None,
            false => Some(EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted,
            }),
        },
        Err(e) => Some(EnrollmentStatus::Error {
            reason: e.to_string(),
        }),
    }
}

pub fn jexl_eval(
    expression_statement: &str,
    targeting_attributes: &TargetingAttributes,
    additional_context: Option<Value>,
) -> Result<bool> {
    let evaluator =
        Evaluator::new().with_transform("versionCompare", |args| Ok(version_compare(args)?));

    let res = match additional_context {
        Some(overrides) => {
            let defaults = serde_json::to_value(targeting_attributes)?;
            let ctx = overrides.defaults(&defaults)?;
            evaluator.eval_in_context(expression_statement, ctx)?
        }
        None => {
            let ctx = targeting_attributes;
            evaluator.eval_in_context(expression_statement, ctx)?
        }
    };
    match res.as_bool() {
        Some(v) => Ok(v),
        None => Err(NimbusError::InvalidExpression),
    }
}

use crate::versioning::Version;

fn version_compare(args: &[Value]) -> Result<Value> {
    let curr_version = args.get(0).ok_or_else(|| {
        NimbusError::VersionParsingError("current version doesn't exist in jexl transform".into())
    })?;
    let curr_version = curr_version.as_str().ok_or_else(|| {
        NimbusError::VersionParsingError("current version in jexl transform is not a string".into())
    })?;
    let min_version = args.get(1).ok_or_else(|| {
        NimbusError::VersionParsingError("minimum version doesn't exist in jexl transform".into())
    })?;
    let min_version = min_version.as_str().ok_or_else(|| {
        NimbusError::VersionParsingError("minium version is not a string in jexl transform".into())
    })?;
    let min_version = Version::try_from(min_version)?;
    let curr_version = Version::try_from(curr_version)?;
    Ok(json!(if curr_version > min_version {
        1
    } else if curr_version < min_version {
        -1
    } else {
        0
    }))
}
