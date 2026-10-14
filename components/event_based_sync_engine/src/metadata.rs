/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Engine-owned persistent metadata.
//!
//! Sync bookkeeping the engine persists across restarts: the per-field last-seen
//! map `[GateKey -> EventPosition]`, the server records seen, the last sync time,
//! and the sync association. The trait keeps the engine storage-agnostic. This
//! module has an in-memory impl for tests and examples; the SQLite one
//! (`SqliteMetadataStore`) is in `metadata_sql`, behind the `sql` feature.

use crate::baseline::BaselineState;
use crate::error::Result;
use crate::event::{EventPosition, GateKey};
use std::collections::HashMap;
use std::sync::Mutex;
use sync15::engine::EngineSyncAssociation;
use sync15::{Guid, ServerTimestamp};

/// Persistence of the engine's own sync metadata.
pub trait MetadataStore: Send + Sync {
    /// The last-seen position for a gate key, if any.
    fn position(&self, key: &GateKey) -> Result<Option<EventPosition>>;

    /// Record the last-seen position for a gate key.
    fn set_position(&self, key: GateKey, pos: EventPosition) -> Result<()>;

    /// Compaction: drop every last-seen entry strictly older than
    /// `covers_up_to` (the position covered by a new baseline).
    fn prune_positions(&self, covers_up_to: &EventPosition) -> Result<()>;

    /// Remember server record ids, each under the position of the event it
    /// belongs to (so every frame of a framed event shares one position).
    fn track_records(&self, records: &[(Guid, EventPosition)]) -> Result<()>;

    /// Tracked record ids whose event is at or below `covers_up_to`, sorted.
    fn records_covered_by(&self, covers_up_to: &EventPosition) -> Result<Vec<Guid>>;

    /// Stop tracking these record ids.
    fn forget_records(&self, ids: &[Guid]) -> Result<()>;

    /// Last sync time. Milliseconds since the epoch; `EPOCH` for "never".
    fn last_sync(&self) -> Result<ServerTimestamp>;
    fn set_last_sync(&self, ts: ServerTimestamp) -> Result<()>;

    /// The collection's sync association (sync IDs), or `Disconnected`.
    fn sync_assoc(&self) -> Result<EngineSyncAssociation>;
    fn set_sync_assoc(&self, assoc: EngineSyncAssociation) -> Result<()>;

    /// Baseline bookkeeping (floor, newest event, cadence counters).
    fn baseline_state(&self) -> Result<BaselineState>;
    fn set_baseline_state(&self, state: BaselineState) -> Result<()>;

    /// Drop all sync state. Used by `reset`/`wipe`.
    fn reset(&self) -> Result<()>;
}

/// In-memory [`MetadataStore`], for tests and examples.
#[derive(Default)]
pub struct MemoryMetadataStore {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    positions: HashMap<GateKey, EventPosition>,
    records: HashMap<Guid, EventPosition>,
    last_sync: Option<ServerTimestamp>,
    assoc: Option<EngineSyncAssociation>,
    baseline: BaselineState,
}

impl MemoryMetadataStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MetadataStore for MemoryMetadataStore {
    fn position(&self, key: &GateKey) -> Result<Option<EventPosition>> {
        Ok(self.inner.lock().unwrap().positions.get(key).cloned())
    }

    fn set_position(&self, key: GateKey, pos: EventPosition) -> Result<()> {
        self.inner.lock().unwrap().positions.insert(key, pos);
        Ok(())
    }

    fn prune_positions(&self, covers_up_to: &EventPosition) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .positions
            .retain(|_, pos| pos.supersedes(covers_up_to) || pos == covers_up_to);
        Ok(())
    }

    fn track_records(&self, records: &[(Guid, EventPosition)]) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        for (id, pos) in records {
            inner.records.insert(id.clone(), pos.clone());
        }
        Ok(())
    }

    fn records_covered_by(&self, covers_up_to: &EventPosition) -> Result<Vec<Guid>> {
        let inner = self.inner.lock().unwrap();
        let mut ids: Vec<Guid> = inner
            .records
            .iter()
            .filter(|(_, pos)| !pos.supersedes(covers_up_to))
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        Ok(ids)
    }

    fn forget_records(&self, ids: &[Guid]) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        for id in ids {
            inner.records.remove(id);
        }
        Ok(())
    }

    fn last_sync(&self) -> Result<ServerTimestamp> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .last_sync
            .unwrap_or(ServerTimestamp::EPOCH))
    }

    fn set_last_sync(&self, ts: ServerTimestamp) -> Result<()> {
        self.inner.lock().unwrap().last_sync = Some(ts);
        Ok(())
    }

    fn sync_assoc(&self) -> Result<EngineSyncAssociation> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .assoc
            .clone()
            .unwrap_or(EngineSyncAssociation::Disconnected))
    }

    fn set_sync_assoc(&self, assoc: EngineSyncAssociation) -> Result<()> {
        self.inner.lock().unwrap().assoc = Some(assoc);
        Ok(())
    }

    fn baseline_state(&self) -> Result<BaselineState> {
        Ok(self.inner.lock().unwrap().baseline.clone())
    }

    fn set_baseline_state(&self, state: BaselineState) -> Result<()> {
        self.inner.lock().unwrap().baseline = state;
        Ok(())
    }

    fn reset(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.positions.clear();
        inner.records.clear();
        inner.last_sync = None;
        inner.assoc = None;
        inner.baseline = BaselineState::default();
        Ok(())
    }
}
