/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This might be where the bucketing logic can go
//! It would be different from current experimentation tools
//! There is a namespacing concept to allow users to be in multiple
//! unrelated experiments at the same time.

//! TODO: Implement the bucketing logic from the nimbus project

use crate::error::{Error, Result};
use crate::{matcher::AppContext, sampling};
use crate::{Branch, EnrolledExperiment, Experiment};
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

/// Filter incoming experiments and enroll users in the appropriate branch
///
/// # Arguments:
///
/// - `id` The user's id
/// - `experiments` A list of experiments, usually retrieved from the network or persisted storage
///
/// # Returns:
///
/// Returns a list of `EnrolledExperiments` that only includes experiments that the user is enrolled in
/// the `EnrolledExperiments` struct contains a `branch` member to indicate the branch chosen for
/// the user.
///
/// # Errors:
///
/// The function can return errors in one of the following cases (but not limited to):
///
/// - If the bucket sampling failed (i.e we could not find if the user should or should not be enrolled in the experiment based on the bucketing)
/// - If an error occurs while determining the branch the user should be enrolled in any of the experiments
#[allow(dead_code)]
pub(crate) fn filter_enrolled(
    id: &Uuid,
    experiments: &[Experiment],
) -> Result<Vec<EnrolledExperiment>> {
    let mut res = Vec::with_capacity(experiments.len());
    for exp in experiments {
        let bucket_config = exp.arguments.bucket_config.clone();
        if sampling::bucket_sample(
            vec![id.to_string(), bucket_config.namespace],
            bucket_config.start,
            bucket_config.count,
            bucket_config.total,
        )? {
            res.push(EnrolledExperiment {
                slug: exp.arguments.slug.clone(),
                user_facing_name: exp.arguments.user_facing_name.clone(),
                user_facing_description: exp.arguments.user_facing_description.clone(),
                branch_slug: choose_branch(&exp.arguments.slug, &exp.arguments.branches, id)?
                    .clone()
                    .slug,
            });
        }
    }
    Ok(res)
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
    id: &Uuid,
) -> Result<&'a Branch> {
    let ratios = branches.iter().map(|b| b.ratio).collect::<Vec<_>>();
    // Note: The "experiment-manager" here comes from https://searchfox.org/mozilla-central/source/toolkit/components/messaging-system/experiments/ExperimentManager.jsm#421
    // TODO: Change it to be something more related to the SDK if it is needed
    let input = format!("{:}-{:}-{:}-branch", "experiment-manager", id, slug);
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
/// Returns true if the user is targeted by the expriment, false otherwise
///
/// # Errors
///
/// Returns errors in the following cases (But not limited to):
/// - The `expression_statement` is not a valid JEXL statement
/// - The `expression_statement` expects fields that do not exist in the AppContext definition
/// - The result of evaluating the statement against the context is not a boolean
/// - jexl-rs returned an error
#[allow(unused)]
pub(crate) fn targeting(expression_statement: &str, ctx: AppContext) -> Result<bool> {
    let res = Evaluator::new().eval_in_context(expression_statement, ctx)?;
    res.as_bool().ok_or(Error::InvalidExpression)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BucketConfig, ExperimentArguments, RandomizationUnit};
    #[test]
    fn test_targeting() {
        // Here's our valid jexl statement
        let expression_statement =
            "app_id == '1010' && (app_version == '4.4' || locale == \"en-US\")";

        // A matching context testing the logical AND + OR of the expression
        let ctx = AppContext {
            app_id: Some("1010".to_string()),
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
        assert!(targeting(expression_statement, ctx).unwrap());

        // A matching context testing the logical OR of the expression
        let ctx = AppContext {
            app_id: Some("1010".to_string()),
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
        assert!(targeting(expression_statement, ctx).unwrap());

        // A non-matching context testing the logical AND of the expression
        let non_matching_ctx = AppContext {
            app_id: Some("org.example.app".to_string()),
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
        assert!(!targeting(expression_statement, non_matching_ctx).unwrap());

        // A non-matching context testing the logical OR of the expression
        let non_matching_ctx = AppContext {
            app_id: Some("org.example.app".to_string()),
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
        assert!(!targeting(expression_statement, non_matching_ctx).unwrap());
    }

    #[test]
    #[should_panic(expected = "EvaluationError")]
    fn test_invalid_expression() {
        // This is an invlalid JEXL statement
        let expression_statement = "This is not a valid JEXL expression";

        // A dummy context, we are really only interested in checking the
        // expression in this test.
        let ctx = AppContext {
            app_id: Some("com.example.app".to_string()),
            app_version: None,
            app_build: None,
            architecture: None,
            device_manufacturer: None,
            device_model: None,
            locale: None,
            os: None,
            os_version: None,
            android_sdk_version: None,
            debug_tag: None,
        };
        targeting(expression_statement, ctx).unwrap();
    }

    #[test]
    fn test_choose_branch() {
        let slug = "TEST_EXP1";
        let branches = vec![
            Branch {
                slug: "control".to_string(),
                group: None,
                ratio: 1,
                value: None,
            },
            Branch {
                slug: "blue".to_string(),
                group: None,
                ratio: 1,
                value: None,
            },
        ];
        // 299eed1e-be6d-457d-9e53-da7b1a03f10d maps to the second index
        let id = uuid::Uuid::parse_str("299eed1e-be6d-457d-9e53-da7b1a03f10d").unwrap();
        let b = choose_branch(slug, &branches, &id).unwrap();
        assert_eq!(b.slug, "blue");
        // 542213c0-9aef-47eb-bc6b-3b8529736ba2 maps to the first index
        let id = uuid::Uuid::parse_str("542213c0-9aef-47eb-bc6b-3b8529736ba2").unwrap();
        let b = choose_branch(slug, &branches, &id).unwrap();
        assert_eq!(b.slug, "control");
    }

    #[test]
    fn test_filter_enrolled() {
        let experiment1 = Experiment {
            id: "ID_1".to_string(),
            filter_expression: "".to_string(),
            targeting: Default::default(),
            enabled: true,
            arguments: ExperimentArguments {
                slug: "TEST_EXP1".to_string(),
                user_facing_name: Default::default(),
                user_facing_description: Default::default(),
                active: true,
                is_enrollment_paused: false,
                bucket_config: BucketConfig {
                    randomization_unit: RandomizationUnit::NormandyId,
                    namespace: "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77".to_string(),
                    start: 0,
                    count: 2000,
                    total: 10000,
                },
                features: Default::default(),
                branches: vec![Branch {slug: "control".to_string(), group: None, ratio: 1, value: None},
                Branch {slug: "blue".to_string(), group: None, ratio: 1, value: None}],
                start_date: serde_json::from_str("\"2020-06-17T23:20:47.230Z\"").unwrap(),
                end_date: Default::default(),
                proposed_duration: Default::default(),
                proposed_enrollment: Default::default(),
                reference_branch: Some("control".to_string())
            },
        };
        let mut experiment2 = experiment1.clone();
        experiment2.arguments.bucket_config = BucketConfig {
            randomization_unit: RandomizationUnit::NormandyId,
            namespace:
                "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77"
                    .to_string(),
            start: 2000,
            count: 3000,
            total: 10000,
        };
        experiment2.arguments.slug = "TEST_EXP2".to_string();
        let experiments = vec![experiment1, experiment2];
        // 299eed1e-be6d-457d-9e53-da7b1a03f10d uuid fits in start: 0, count: 2000, total: 10000 with the example namespace, to the treatment-variation-b branch
        // Tested against the desktop implementation
        let id = uuid::Uuid::parse_str("299eed1e-be6d-457d-9e53-da7b1a03f10d").unwrap();
        let enrolled = filter_enrolled(&id, &experiments).unwrap();
        assert_eq!(enrolled.len(), 1);
        assert_eq!(enrolled[0].slug, "TEST_EXP1");
        // 542213c0-9aef-47eb-bc6b-3b8529736ba2 uuid fits in start: 2000, count: 3000, total: 10000 with the example namespace, to the control branch
        // Tested against the desktop implementation
        let id = uuid::Uuid::parse_str("542213c0-9aef-47eb-bc6b-3b8529736ba2").unwrap();
        let enrolled = filter_enrolled(&id, &experiments).unwrap();
        assert_eq!(enrolled.len(), 1);
        assert_eq!(enrolled[0].slug, "TEST_EXP2");
    }
}
