/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! This module implements the primitive functions to implement
//! safe updating from the server.

use crate::error::Result;
use crate::stateful::persistence::{Database, StoreId, Writer};
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
