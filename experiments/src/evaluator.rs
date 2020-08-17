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
            "app_id == '1010' && ( app_version == '4.4' || locale == \"en-US\")";
        // A valid context
        let ctx = AppContext {
            app_id: Some("1010".to_string()),
            app_version: Some("4.4".to_string()),
            locale: Some("en-US".to_string()),
            debug_tag: None,
            device_manufacturer: None,
            device_model: None,
            region: None,
        };
        assert!(targeting(expression_statement, ctx).unwrap())
    }
}

// TODO: Implement unit testing for the bucketing logic based on the Nimbus requirments
