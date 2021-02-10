/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Testing get_experiment_branch semantics.

mod common;
use nimbus::error::{Error, Result};

#[cfg(feature = "rkv-safe-mode")]
#[test]
fn test_before_open() -> Result<()> {
    let _ = env_logger::try_init();
    let client = common::new_test_client("test_before_open")?;
    assert!(matches!(
        client.get_experiment_branch("foo".to_string()),
        Err(Error::DatabaseNotReady)
    ));
    // now initialize the DB - it should start working (and report no branch)
    client.initialize()?;
    assert_eq!(client.get_experiment_branch("foo".to_string())?, None);
    Ok(())
}

#[cfg(feature = "rkv-safe-mode")]
#[test]
fn test_enrolled() -> Result<()> {
    let _ = env_logger::try_init();
    let client = common::new_test_client("test_before_open")?;
    client.initialize()?;
    client.set_experiments_locally(common::initial_test_experiments())?;

    // haven't applied them yet, so not enrolled as the experiment doesn't
    // really exist.
    assert_eq!(
        client.get_experiment_branch("secure-gold".to_string())?,
        None
    );
    client.apply_pending_experiments()?;
    // secure-gold enrolls everyone - not clear what treatment though.
    assert!(client
        .get_experiment_branch("secure-gold".to_string())?
        .is_some());

    client.opt_out("secure-gold".to_string())?;
    assert_eq!(
        client.get_experiment_branch("secure-gold".to_string())?,
        None
    );

    client.opt_in_with_branch("secure-gold".to_string(), "treatment".to_string())?;
    assert_eq!(
        client.get_experiment_branch("secure-gold".to_string())?,
        Some("treatment".to_string())
    );

    client.opt_in_with_branch("secure-gold".to_string(), "control".to_string())?;
    assert_eq!(
        client.get_experiment_branch("secure-gold".to_string())?,
        Some("control".to_string())
    );

    client.set_global_user_participation(false)?;
    assert_eq!(
        client.get_experiment_branch("secure-gold".to_string())?,
        None
    );
    Ok(())
}
