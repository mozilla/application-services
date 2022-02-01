/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Testing get_experiment_branch semantics.

mod common;
#[allow(unused_imports)]
#[allow(unused_attributes)]
#[macro_use]
use nimbus::error::Result;



#[cfg(feature = "rkv-safe-mode")]
#[cfg(test)]
mod message_tests {

    use nimbus::{AppContext, TargetingAttributes};
    use serde_json::json;

    use super::*;

    #[test]
    fn test_jexl_expression() -> Result<()> {

        let nimbus = common::new_test_client("jexl_test")?;
        nimbus.initialize()?;

        let helper = nimbus.create_targeting_helper();

        // We get a boolean back from a string!
        assert!(
            helper.eval_jexl("app_name == 'fenix'".to_string(), None)?
        );

        // We get true and false back from two similar JEXL expressions!
        // I think we can convince ourselves that JEXL is being evaluated against the
        // AppContext.
        assert!(
            !helper.eval_jexl("app_name == 'xinef'".to_string(), None)?
        );

        // The expression contains a variable not declared (snek_case Good, camelCase Bad)
        assert!(
            helper.eval_jexl("appName == 'fenix'".to_string(), None).is_err()
        );

        // Check the versionCompare function, just to prove to ourselves that it's the same JEXL evaluator.
        assert!(
            helper.eval_jexl(
                "(version|versionCompare('95.!') >= 0) && (version|versionCompare('96.!') < 0)".to_string(),
                json!({
                    "version": "95.1.0".to_string(),
                }).as_object().cloned(),
            )?
        );

        assert!(
            !helper.eval_jexl(
                "(version|versionCompare('95.!') >= 0) && (version|versionCompare('96.!') < 0)".to_string(),
                json!({
                    "version": "94.0.0".to_string(),
                }).as_object().cloned(),
            )?
        );

        Ok(())
    }

    #[test]
    fn test_jexl_expression_with_targeting_attributes() -> Result<()> {
        let mut nimbus = common::new_test_client("jexl_test_days_since")?;
        nimbus.initialize()?;

        let helper = nimbus.create_targeting_helper();

        assert!(
            helper.eval_jexl("days_since_install == 0".to_string(), None)?
        );

        assert!(
            helper.eval_jexl("days_since_update == 0".to_string(), None)?
        );

        let app_context = AppContext {
            app_name: "fenix".to_string(),
            app_id: "org.mozilla.fenix".to_string(),
            channel: "nightly".to_string(),
            ..Default::default()
        };

        let targeting_attributes = TargetingAttributes {
            app_context,
            days_since_install: Some(10),
            days_since_update: Some(5),
            is_already_enrolled: false,
        };

        nimbus.with_targeting_attributes(targeting_attributes);

        let helper = nimbus.create_targeting_helper();
        assert!(
            helper.eval_jexl("days_since_install == 10".to_string(), None)?
        );

        assert!(
            helper.eval_jexl("days_since_update == 5".to_string(), None)?
        );

        Ok(())
    }
}