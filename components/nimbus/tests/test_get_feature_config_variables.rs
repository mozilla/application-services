/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Testing featured-based get_experiment_branch semantics.

mod common;
#[allow(unused_imports)]
use nimbus::error::Result;

#[cfg(feature = "rkv-safe-mode")]
#[test]
fn test_enrolled_feature2() -> Result<()> {
    let _ = env_logger::try_init();
    let client = common::new_test_client("test_enrolled_feature2")?;
    client.initialize()?;
    client.set_experiments_locally(common::experiments_testing_feature_ids())?;

    client.apply_pending_experiments()?;

    // client.opt_out("secure-gold".to_string())?;
    // assert_eq!(
    //     client.get_experiment_branch("aboutwelcome".to_string())?,
    //     None,
    //     "should not return a branch we've just opted out of"
    // );

    // client.opt_in_with_branch("secure-gold".to_string(), "treatment".to_string())?;
    // assert_eq!(
    //     client.get_experiment_branch("aboutwelcome".to_string())?,
    //     Some("treatment".to_string()),
    //     "should return a treatment branch we've just opted into"
    // );

    // XXX variables in getter versus value in schema
    client.opt_in_with_branch("secure-gold".to_string(), "control".to_string())?;
    assert_eq!(
        client.get_feature_config_variables("aboutwelcome".to_string())?.unwrap(),
        r#"{"welcome_string": "hi"}"#,
        "should return the right feature_config value 'control' branch"
    );

    client.opt_in_with_branch("secure-gold".to_string(), "treatment".to_string())?;
    assert_eq!(
        client.get_feature_config_variables("aboutwelcome".to_string())?.unwrap(),
        r#"{"welcome_string": "hello"}"#,
        "should return the right feature_config value for 'treatment' branch"
    );

    // assert_eq!(
    //     client.get_experiment_branch("aboutwelcome".to_string())?,
    //     Some("control".to_string()),
    //     "should return a second branch we've just opted into"
    // );

    // client.set_global_user_participation(false)?;
    // assert_eq!(
    //     client.get_experiment_branch("aboutwelcome".to_string())?,
    //     None,
    //     "should not return a branch if we've globally opted out"
    // );

    Ok(())
}
