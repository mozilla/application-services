/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Utilities for engine tests.
use super::SyncEngine;
use crate::bso::IncomingBso;
use crate::telemetry;

/// Stages `incoming` with one `stage_incoming()` call per `batch_size` records
/// the way the sync client does when it downloads a collection a page at a time.
/// Tests that pass `batch_size = 1` check that an engine doesn't assume all its
/// records arrive in a single call. An empty `incoming` makes no calls at all;
/// tests that need an empty `stage_incoming()` call should make it directly.
pub fn stage_in_batches<E: SyncEngine + ?Sized>(
    engine: &E,
    incoming: Vec<IncomingBso>,
    batch_size: usize,
    telem: &mut telemetry::Engine,
) -> anyhow::Result<()> {
    assert!(batch_size > 0, "batch_size must be greater than zero");
    let mut incoming = incoming.into_iter().peekable();
    while incoming.peek().is_some() {
        let batch: Vec<_> = incoming.by_ref().take(batch_size).collect();
        engine.stage_incoming(batch, telem)?;
    }
    Ok(())
}
