/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This might be where the bucketing logic can go
//! It would be different from current experimentation tools
//! There is a namespacing concept to allow users to be in multiple
//! unrelated experiments at the same time.

//! TODO: Implement the bucketing logic from the nimbus project

use crate::error::{Error, Result};
use crate::matcher::AppContext;
use jexl_eval::Evaluator;
use serde_derive::*;
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Bucket {}

impl Bucket {
    #[allow(unused)]
    pub fn new() -> Self {
        unimplemented!();
    }
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
/// Returns errors in the following cases (But not limited to):
/// - The `expression_statement` is not a valid JEXL statement
/// - The `expression_statement` expects fields that do not exist in the AppContext definition
/// - The result of evaluating the statement against the context is not a boolean
/// - jexl-rs returned an error
#[allow(unused)]
pub fn targeting(expression_statement: &str, ctx: AppContext) -> Result<bool> {
    let res = Evaluator::new().eval_in_context(expression_statement, ctx)?;
    res.as_bool().ok_or(Error::InvalidExpression)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let expression_statement =
            "This is not a valid JEXL expression";

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
}

// TODO: Implement unit testing for the bucketing logic based on the Nimbus requirments
