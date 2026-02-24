/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Allow this to run in "safe mode"
#![allow(unused_imports)]

use crate::Experiment;
use crate::error::Result;
use crate::stateful::persistence::Database;
use crate::stateful::updating::*;

// This test crashes lmdb for reasons that make no sense, so only run it
// in the "safe mode" backend.
#[cfg(feature = "rkv-safe-mode")]
#[test]
fn test_reading_writing_and_removing_experiments() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;

    error_support::init_for_tests();

    let test_experiment: Experiment = Default::default();
    let fetched = vec![test_experiment];

    // simulated fetch by constructing a dummy payload of 1 experiment.
    assert_eq!(fetched.len(), 1);

    write_pending_experiments(&db, &mut writer, fetched)?;

    // Now, we come to get the stashed updates, and they should be
    // the same.
    let pending = read_and_remove_pending_experiments(&db, &mut writer)?;

    assert_eq!(pending.unwrap().len(), 1);

    // After we've fetched this once, we should have no pending
    // updates left.
    let pending = read_and_remove_pending_experiments(&db, &mut writer)?;

    assert!(pending.is_none(), "No pending updates should be stashed");

    writer.commit()?;
    Ok(())
}
