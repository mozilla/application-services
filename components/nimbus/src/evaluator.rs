/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

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
    // Verify the app_name matches the application being targeted
    // by the experiment.
    match &exp.app_name {
        Some(app_name) => {
            if !app_name.eq(&app_context.app_name) {
                return Ok(ExperimentEnrollment {
                    slug: exp.slug.clone(),
                    status: EnrollmentStatus::NotEnrolled {
                        reason: NotEnrolledReason::NotTargeted,
                    },
                });
            }
        }
        None => log::debug!("Experiment missing app_name, skipping it as a targeting parameter"),
    }
    // Verify the app_id matches the application being targeted
    // by the experiment.
    match &exp.app_id {
        Some(app_id) => {
            if !app_id.eq(&app_context.app_id) {
                return Ok(ExperimentEnrollment {
                    slug: exp.slug.clone(),
                    status: EnrollmentStatus::NotEnrolled {
                        reason: NotEnrolledReason::NotTargeted,
                    },
                });
            }
        }
        None => log::debug!("Experiment missing app_id, skipping it as a targeting parameter"),
    }
    // Verify the channel matches the application being targeted
    // by the experiment.  Note, we are intentionally comparing in a case-insensitive way.
    // See https://jira.mozilla.com/browse/SDK-246 for more info.
    match &exp.channel {
        Some(channel) => {
            if !channel.to_lowercase().eq(&app_context.channel.to_lowercase()) {
                return Ok(ExperimentEnrollment {
                    slug: exp.slug.clone(),
                    status: EnrollmentStatus::NotEnrolled {
                        reason: NotEnrolledReason::NotTargeted,
                    },
                });
            }
        }
        None => log::debug!("Experiment missing channel, skipping it as a targeting parameter"),
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
                            &exp.get_first_feature_id(),
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
    branches.get(index).ok_or(NimbusError::OutOfBoundsError)
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
                reason: NimbusError::InvalidExpression.to_string(),
            }),
        },
        Err(e) => Some(EnrollmentStatus::Error {
            reason: NimbusError::EvaluationError(e.to_string()).to_string(),
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
            app_name: "nimbus_test".to_string(),
            app_id: "1010".to_string(),
            channel: "test".to_string(),
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
            app_name: "nimbus_test".to_string(),
            app_id: "1010".to_string(),
            channel: "test".to_string(),
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
            app_name: "not_nimbus_test".to_string(),
            app_id: "org.example.app".to_string(),
            channel: "test".to_string(),
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
            app_name: "not_nimbus_test".to_string(),
            app_id: "org.example.app".to_string(),
            channel: "test".to_string(),
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
    fn test_qualified_enrollment() {
        let experiment = Experiment {
            app_name: Some("NimbusTest".to_string()),
            app_id: Some("org.example.app".to_string()),
            channel: Some("nightly".to_string()),
            schema_version: "1.0.0".to_string(),
            slug: "TEST_EXP".to_string(),
            is_enrollment_paused: false,
            feature_ids: vec!["monkey".to_string()],
            bucket_config: BucketConfig {
                randomization_unit: RandomizationUnit::NimbusId,
                start: 0,
                count: 10000,
                total: 10000,
                ..Default::default()
            },
            branches: vec![
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
            ],
            reference_branch: Some("control".to_string()),
            ..Default::default()
        };

        // Application context for matching the above experiment.  If any of the `app_name`, `app_id`,
        // or `channel` doesn't match the experiment, then the client won't be enrolled.
        let mut context = AppContext {
            app_name: "NimbusTest".to_string(),
            app_id: "org.example.app".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };

        let id = uuid::Uuid::new_v4();

        let enrollment = evaluate_enrollment(
            &id,
            &Default::default(),
            &context,
            &experiment,
        )
        .unwrap();
        println!("Uh oh!  {:#?}", enrollment.status);
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::Enrolled {
                reason: EnrolledReason::Qualified,
                ..
            }
        ));

        // Change the channel to test when it has a different case than expected
        // (See SDK-246: https://jira.mozilla.com/browse/SDK-246 )
        context.channel = "Nightly".to_string();

        // Now we will be enrolled in the experiment because we have the right channel, but with different capitalization
        let enrollment =
            evaluate_enrollment(
                &id, &Default::default(),
                &context,
                &experiment,
            ).unwrap();
            assert!(matches!(
                enrollment.status,
                EnrollmentStatus::Enrolled {
                    reason: EnrolledReason::Qualified,
                    ..
                }
            ));
    }

    #[test]
    fn test_wrong_randomization_units() {
        let experiment = Experiment {
            app_name: Some("NimbusTest".to_string()),
            app_id: Some("org.example.app".to_string()),
            channel: Some("nightly".to_string()),
            schema_version: "1.0.0".to_string(),
            slug: "TEST_EXP".to_string(),
            is_enrollment_paused: false,
            feature_ids: vec!["test-feature".to_string()],
            bucket_config: BucketConfig {
                randomization_unit: RandomizationUnit::ClientId,
                start: 0,
                count: 10000,
                total: 10000,
                ..Default::default()
            },
            branches: vec![
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
            ],
            reference_branch: Some("control".to_string()),
            ..Default::default()
        };

        // Application context for matching the above experiment.  If any of the `app_name`, `app_id`,
        // or `channel` doesn't match the experiment, then the client won't be enrolled.
        let context = AppContext {
            app_name: "NimbusTest".to_string(),
            app_id: "org.example.app".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };

        // We won't be enrolled in the experiment because we don't have the right randomization units since the
        // experiment is requesting the `ClientId` and the `Default::default()` here will just have the
        // NimbusId.
        let enrollment = evaluate_enrollment(
            &uuid::Uuid::new_v4(),
            &Default::default(),
            &context,
            &experiment,
        )
        .unwrap();
        // The status should be `Error`
        assert!(matches!(enrollment.status, EnrollmentStatus::Error { .. }));

        // Fits because of the client_id.
        let available_randomization_units = AvailableRandomizationUnits::with_client_id("bobo");
        let id = uuid::Uuid::parse_str("542213c0-9aef-47eb-bc6b-3b8529736ba2").unwrap();
        let enrollment =
            evaluate_enrollment(&id, &available_randomization_units, &context, &experiment)
                .unwrap();
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::Enrolled {
                reason: EnrolledReason::Qualified,
                ..
            }
        ));
    }

    #[test]
    fn test_not_targeted_for_enrollment() {
        let experiment = Experiment {
            app_name: Some("NimbusTest".to_string()),
            app_id: Some("org.example.app".to_string()),
            channel: Some("nightly".to_string()),
            schema_version: "1.0.0".to_string(),
            slug: "TEST_EXP2".to_string(),
            is_enrollment_paused: false,
            feature_ids: vec!["test-feature".to_string()],
            bucket_config: BucketConfig {
                randomization_unit: RandomizationUnit::NimbusId,
                start: 0,
                count: 10000,
                total: 10000,
                ..Default::default()
            },
            branches: vec![
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
            ],
            reference_branch: Some("control".to_string()),
            ..Default::default()
        };

        let id = uuid::Uuid::new_v4();

        // If any of the `app_name`, `app_id`, or `channel` doesn't match the experiment,
        // then the client won't be enrolled.
        // Start with a context that does't match the app_name:
        let mut context = AppContext {
            app_name: "Wrong!".to_string(),
            app_id: "org.example.app".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };

        // We won't be enrolled in the experiment because we don't have the right app_name
        let enrollment =
            evaluate_enrollment(&id, &Default::default(), &context, &experiment).unwrap();
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted
            }
        ));

        // Change the app_name back and change the app_id to test when it doesn't match:
        context.app_name = "NimbusTest".to_string();
        context.app_id = "Wrong".to_string();

        // Now we won't be enrolled in the experiment because we don't have the right app_id, but with the same
        // `NotTargeted` reason
        let enrollment =
            evaluate_enrollment(&id, &Default::default(), &context, &experiment).unwrap();
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted
            }
        ));

        // Change the app_id back and change the channel to test when it doesn't match:
        context.app_id = "org.example.app".to_string();
        context.channel = "Wrong".to_string();

        // Now we won't be enrolled in the experiment because we don't have the right channel, but with the same
        // `NotTargeted` reason
        let enrollment =
            evaluate_enrollment(&id, &Default::default(), &context, &experiment).unwrap();
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted
            }
        ));
    }

    #[test]
    fn test_enrollment_bucketing() {
        let experiment = Experiment {
            app_id: Some("org.example.app".to_string()),
            channel: Some("nightly".to_string()),
            schema_version: "1.0.0".to_string(),
            slug: "TEST_EXP1".to_string(),
            is_enrollment_paused: false,
            feature_ids: vec!["test-feature".to_string()],
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

        let available_randomization_units = Default::default();
        // 299eed1e-be6d-457d-9e53-da7b1a03f10d uuid fits in start: 0, count: 2000, total: 10000 with the example namespace, to the treatment-variation-b branch
        // Tested against the desktop implementation
        let id = uuid::Uuid::parse_str("299eed1e-be6d-457d-9e53-da7b1a03f10d").unwrap();
        // Application context for matching exp3
        let context = AppContext {
            app_id: "org.example.app".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };

        let enrollment =
            evaluate_enrollment(&id, &available_randomization_units, &context, &experiment)
                .unwrap();
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::Enrolled {
                reason: EnrolledReason::Qualified,
                ..
            }
        ));
    }
}
