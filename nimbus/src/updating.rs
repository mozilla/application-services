/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module implements the primitive functions to implement
//! safe updating from the server.

use crate::error::Result;
use crate::persistence::{Database, StoreId, Writer};
use crate::Experiment;

const KEY_PENDING_UPDATES: &str = "pending-experiment-updates";

pub fn write_pending_experiments(
    db: &Database,
    writer: &mut Writer,
    experiments: Vec<Experiment>,
) -> Result<()> {
    db.get_store(StoreId::Updates)
        .put(writer, KEY_PENDING_UPDATES, &experiments)
}

pub fn read_and_remove_pending_experiments(
    db: &Database,
    writer: &mut Writer,
) -> Result<Option<Vec<Experiment>>> {
    let store = db.get_store(StoreId::Updates);
    let experiments = store.get::<Vec<Experiment>, _>(writer, KEY_PENDING_UPDATES)?;

    // Only clear the store if there's updates available.
    // If we're accidentally called from the main thread,
    // we don't want to be writing unless we absolutely have to.
    if experiments.is_some() {
        store.clear(writer)?;
    }

    // An empty Some(vec![]) is "updates of an empty list" i.e. unenrolling from all experiments
    // None is "there are no pending updates".
    Ok(experiments)
}

// This test crashes lmdb for reasons that make no sense, so only run it
// in the "safe mode" backend.
#[cfg(feature = "rkv-safe-mode")]
#[test]
fn test_reading_writing_and_removing_experiments() -> Result<()> {
    use crate::Experiment;
    use tempdir::TempDir;

    let tmp_dir = TempDir::new("test_stash_pop_updates")?;
    let db = Database::new(&tmp_dir)?;
    let mut writer = db.write()?;

    let _ = env_logger::try_init();

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
