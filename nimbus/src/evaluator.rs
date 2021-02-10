/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use crate::enrollment::{
    EnrolledReason, EnrollmentStatus, ExperimentEnrollment, NotEnrolledReason,
};
use crate::{
    error::{Error, Result},
    AvailableRandomizationUnits,
};
use crate::{matcher::AppContext, sampling};
use crate::{Branch, Experiment};
use jexl_eval::Evaluator;
use serde_derive::*;
use uuid::Uuid;
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Bucket {}

impl Bucket {
    #[allow(unused)]
    pub fn new() -> Self {
        unimplemented!();
    }
}

/// Determine the enrolment status for an experiment.
///
/// # Arguments:
///
/// - `nimbus_id` The auto-generated nimbus_id
/// - `available_randomization_units`: The app provded available randomization units
/// - `experiment` - The experiment.
///
/// An `ExperimentEnrollment` -  you need to inspect the EnrollmentStatus to
/// determine if the user is actually enrolled.
/// # Errors:
///
/// The function can return errors in one of the following cases (but not limited to):
///
/// - If the bucket sampling failed (i.e we could not find if the user should or should not be enrolled in the experiment based on the bucketing)
/// - If an error occurs while determining the branch the user should be enrolled in any of the experiments
pub fn evaluate_enrollment(
    nimbus_id: &Uuid,
    available_randomization_units: &AvailableRandomizationUnits,
    app_context: &AppContext,
    exp: &Experiment,
) -> Result<ExperimentEnrollment> {
    // Verify the application-id matches the application being targeted
    // by the experiment.
    if !exp.application.eq(&app_context.app_id) {
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
        if let Some(status) = targeting(expr, app_context) {
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
                            &choose_branch(&exp.slug, &exp.branches, &id)?.clone().slug,
                            &exp.feature_ids[0],
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
fn choose_branch<'a>(slug: &str, branches: &'a [Branch], id: &str) -> Result<&'a Branch> {
    // convert from i32 to u32 to work around SDK-175.
    let ratios = branches.iter().map(|b| b.ratio as u32).collect::<Vec<_>>();
    // Note: The "experiment-manager" here comes from
    // https://searchfox.org/mozilla-central/rev/1843375acbbca68127713e402be222350ac99301/toolkit/components/messaging-system/experiments/ExperimentManager.jsm#469
    // TODO: Change it to be something more related to the SDK if it is needed
    let input = format!("{:}-{:}-{:}-branch", "experimentmanager", id, slug);
    let index = sampling::ratio_sample(&input, &ratios)?;
    branches.get(index).ok_or(Error::OutOfBoundsError)
}

/// Checks if the client is targeted by an experiment
/// This api evaluates the JEXL statement retrieved from the server
/// against the application context provided by the client
///
/// # Arguments
/// - `expression_statement`: The JEXL statement provided by the server
/// - `ctx`: The application context provided by the client
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
fn targeting(expression_statement: &str, ctx: &AppContext) -> Option<EnrollmentStatus> {
    match Evaluator::new().eval_in_context(expression_statement, ctx.clone()) {
        Ok(res) => match res.as_bool() {
            Some(true) => None,
            Some(false) => Some(EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted,
            }),
            None => Some(EnrollmentStatus::Error {
                reason: Error::InvalidExpression.to_string(),
            }),
        },
        Err(e) => Some(EnrollmentStatus::Error {
            reason: Error::EvaluationError(e.to_string()).to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BucketConfig, Experiment, RandomizationUnit};

    #[test]
    fn test_targeting() {
        // Here's our valid jexl statement
        let expression_statement =
            "app_id == '1010' && (app_version == '4.4' || locale == \"en-US\")";

        // A matching context testing the logical AND + OR of the expression
        let ctx = AppContext {
            app_id: "1010".to_string(),
            app_version: Some("4.4".to_string()),
            app_build: Some("1234".to_string()),
            architecture: Some("x86_64".to_string()),
            device_manufacturer: Some("Samsung".to_string()),
            device_model: Some("Galaxy S10".to_string()),
            locale: Some("en-US".to_string()),
            os: Some("Android".to_string()),
            os_version: Some("10".to_string()),
            android_sdk_version: Some("29".to_string()),
            debug_tag: None,
        };
        assert_eq!(targeting(expression_statement, &ctx), None);

        // A matching context testing the logical OR of the expression
        let ctx = AppContext {
            app_id: "1010".to_string(),
            app_version: Some("4.4".to_string()),
            app_build: Some("1234".to_string()),
            architecture: Some("x86_64".to_string()),
            device_manufacturer: Some("Samsung".to_string()),
            device_model: Some("Galaxy S10".to_string()),
            locale: Some("de-DE".to_string()),
            os: Some("Android".to_string()),
            os_version: Some("10".to_string()),
            android_sdk_version: Some("29".to_string()),
            debug_tag: None,
        };
        assert_eq!(targeting(expression_statement, &ctx), None);

        // A non-matching context testing the logical AND of the expression
        let non_matching_ctx = AppContext {
            app_id: "org.example.app".to_string(),
            app_version: Some("4.4".to_string()),
            app_build: Some("1234".to_string()),
            architecture: Some("x86_64".to_string()),
            device_manufacturer: Some("Samsung".to_string()),
            device_model: Some("Galaxy S10".to_string()),
            locale: Some("en-US".to_string()),
            os: Some("Android".to_string()),
            os_version: Some("10".to_string()),
            android_sdk_version: Some("29".to_string()),
            debug_tag: None,
        };
        assert!(matches!(
            targeting(expression_statement, &non_matching_ctx),
            Some(EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted
            })
        ));

        // A non-matching context testing the logical OR of the expression
        let non_matching_ctx = AppContext {
            app_id: "org.example.app".to_string(),
            app_version: Some("4.5".to_string()),
            app_build: Some("1234".to_string()),
            architecture: Some("x86_64".to_string()),
            device_manufacturer: Some("Samsung".to_string()),
            device_model: Some("Galaxy S10".to_string()),
            locale: Some("de-DE".to_string()),
            os: Some("Android".to_string()),
            os_version: Some("10".to_string()),
            android_sdk_version: Some("29".to_string()),
            debug_tag: None,
        };
        assert!(matches!(
            targeting(expression_statement, &non_matching_ctx),
            Some(EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted
            })
        ));
    }

    #[test]
    fn test_invalid_expression() {
        // This expression doesn't return a bool
        let expression_statement = "2.0";

        assert_eq!(
            targeting(expression_statement, &Default::default()),
            Some(EnrollmentStatus::Error {
                reason: "Invalid Expression - didn't evaluate to a bool".to_string()
            })
        )
    }

    #[test]
    fn test_evaluation_error() {
        // This is an invalid JEXL statement
        let expression_statement = "This is not a valid JEXL expression";

        assert!(
            matches!(targeting(expression_statement, &Default::default()), Some(EnrollmentStatus::Error { reason }) if reason.starts_with("EvaluationError:"))
        )
    }

    #[test]
    fn test_choose_branch() {
        let slug = "TEST_EXP1";
        let branches = vec![
            Branch {
                slug: "control".to_string(),
                ratio: 1,
                feature: None,
            },
            Branch {
                slug: "blue".to_string(),
                ratio: 1,
                feature: None,
            },
        ];
        // 299eed1e-be6d-457d-9e53-da7b1a03f10d maps to the second index
        let id = uuid::Uuid::parse_str("3d2142de-53bf-2d48-a92d-45fb7036cbf6").unwrap();
        let b = choose_branch(slug, &branches, &id.to_string()).unwrap();
        assert_eq!(b.slug, "blue");
        // 542213c0-9aef-47eb-bc6b-3b8529736ba2 maps to the first index
        let id = uuid::Uuid::parse_str("542213c0-9aef-47eb-bc6b-3b8529736ba2").unwrap();
        let b = choose_branch(slug, &branches, &id.to_string()).unwrap();
        assert_eq!(b.slug, "control");
    }

    #[test]
    fn test_get_enrollment() {
        let experiment1 = Experiment {
            application: "org.example.app".to_string(),
            schema_version: "1.0.0".to_string(),
            slug: "TEST_EXP1".to_string(),
            is_enrollment_paused: false,
            feature_ids: vec!["monkey".to_string()],
            bucket_config: BucketConfig {
                randomization_unit: RandomizationUnit::NimbusId,
                namespace: "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77".to_string(),
                start: 0,
                count: 2000,
                total: 10000,
            },
            branches: vec![
                Branch {slug: "control".to_string(), ratio: 1, feature: None },
                Branch {slug: "blue".to_string(), ratio: 1, feature: None }
            ],
            reference_branch: Some("control".to_string()),
            ..Default::default()
        };

        let mut experiment2 = experiment1.clone();
        experiment2.bucket_config = BucketConfig {
            randomization_unit: RandomizationUnit::ClientId,
            namespace:
                "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77"
                    .to_string(),
            start: 9000,
            count: 1000,
            total: 10000,
        };
        experiment2.slug = "TEST_EXP2".to_string();

        let mut experiment3 = experiment1.clone();
        // We won't match experiment 3 because the application doesn't match.
        experiment3.application = "not.this.app".to_string();
        experiment3.bucket_config = BucketConfig {
            randomization_unit: RandomizationUnit::NimbusId,
            namespace:
                "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77"
                    .to_string(),
            start: 0,
            count: 2000,
            total: 10000,
        };
        experiment3.slug = "TEST_EXP3".to_string();

        // We will not match EXP_2 because we don't have the necessary randomization unit.
        let available_randomization_units = Default::default();
        // 299eed1e-be6d-457d-9e53-da7b1a03f10d uuid fits in start: 0, count: 2000, total: 10000 with the example namespace, to the treatment-variation-b branch
        // Tested against the desktop implementation
        let id = uuid::Uuid::parse_str("299eed1e-be6d-457d-9e53-da7b1a03f10d").unwrap();
        // Application context for matching exp3
        let context = AppContext {
            app_id: "org.example.app".to_string(),
            ..Default::default()
        };

        let enrollment =
            evaluate_enrollment(&id, &available_randomization_units, &context, &experiment1)
                .unwrap();
        assert!(
            matches!(enrollment.status, EnrollmentStatus::Enrolled { reason: EnrolledReason::Qualified, .. })
        );

        let enrollment =
            evaluate_enrollment(&id, &available_randomization_units, &context, &experiment2)
                .unwrap();
        // Don't have the correct randomization_unit
        assert!(matches!(enrollment.status, EnrollmentStatus::Error { .. }));

        let enrollment =
            evaluate_enrollment(&id, &available_randomization_units, &context, &experiment3)
                .unwrap();
        // Doesn't match because it's not the correct application
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted
            }
        ));

        // Fits because of the client_id.
        let available_randomization_units = AvailableRandomizationUnits::with_client_id("bobo");
        let id = uuid::Uuid::parse_str("542213c0-9aef-47eb-bc6b-3b8529736ba2").unwrap();
        let enrollment =
            evaluate_enrollment(&id, &available_randomization_units, &context, &experiment2)
                .unwrap();
        assert!(
            matches!(enrollment.status, EnrollmentStatus::Enrolled { reason: EnrolledReason::Qualified, .. })
        );
    }
}
