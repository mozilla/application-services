/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Baselines: bound the growth of an event stream.
//!
//! Periodically a client writes a **baseline**: one record dumping the full
//! current state, tagged with the position (`covers_up_to`) of the latest event
//! it includes. This module holds the value types; the orchestration lives in
//! [`crate::engine`].
//!
//! A baseline's body is schema-versioned like an event body, under the reserved
//! event type [`SNAPSHOT_EVENT_TYPE`].
//!
//! Not yet implemented: splitting a large baseline across an atomic batch;
//! deleting superseded records from the server; detecting a client that fell
//! behind past compaction.

use crate::event::EventPosition;
use crate::reducer::SnapshotEntity;
use crate::schema::SchemaVersion;
use serde::{Deserialize, Serialize};
use sync15::{Guid, ServerTimestamp};

/// if no baseline lands within this time (~30 days in ms), the highest-schema client
/// still active takes over.
pub const DEFAULT_BASELINE_TIMEOUT_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Default byte budget for a single baseline record before it is split into
/// `_N` parts. Conservative so it holds on any server, with slack for the
/// encryption/base64 overhead applied after we measure the plaintext record.
pub const DEFAULT_MAX_RECORD_PAYLOAD_BYTES: usize = 256 * 1024;

/// Reserved event type under which a baseline snapshot is schema-migrated.
pub const SNAPSHOT_EVENT_TYPE: &str = "__snapshot__";

/// Tuning for automatic baseline generation.
#[derive(Clone, Debug)]
pub struct BaselineConfig {
    /// Cadence. The count starts from the last baseline this client wrote or
    /// saw. When the count is reached but this client is too old to write a
    /// baseline (below the schema bar), it writes a `baseline-needed` marker
    /// instead, and a newer client writes the baseline for it. A client that
    /// sees `baseline-needed` does the same thing. `None` means this client
    /// never starts a baseline on its own, but it still answers a
    /// `baseline-needed` from another client.
    pub events_per_baseline: Option<u64>,
    /// If a needed baseline has been blocked (no capable client available) for
    /// at least this many ms of server time, the highest-schema client still
    /// syncing writes one anyway. `None` disables the override.
    pub baseline_timeout_ms: Option<i64>,
    /// Byte budget for a single baseline record. A baseline larger than this is
    /// split into `_N` parts, each under the budget (see [`split_snapshot`]).
    pub max_record_payload_bytes: usize,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        // Count-based cadence: a baseline roughly every 500 events.
        Self {
            events_per_baseline: Some(500),
            baseline_timeout_ms: Some(DEFAULT_BASELINE_TIMEOUT_MS),
            max_record_payload_bytes: DEFAULT_MAX_RECORD_PAYLOAD_BYTES,
        }
    }
}

/// The engine's persistent baseline bookkeeping.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BaselineState {
    /// Position covered by the most recent baseline. Events at or below the
    /// floor are superseded and ignored. `None` before any baseline is known.
    pub floor: Option<EventPosition>,
    /// Newest event position observed; candidate `covers_up_to` for a baseline
    /// this client might write.
    pub newest_event: Option<EventPosition>,
    /// Events observed since the last baseline (drives the cadence).
    pub events_since_baseline: u64,
    /// Highest schema version seen since the last baseline. A client may only
    /// write a baseline if its current schema is at least this high.
    pub max_schema_since_baseline: SchemaVersion,
    /// Server time this client first found a needed baseline blocked by the
    /// schema bar. Drives the [`BaselineConfig::baseline_timeout_ms`] override.
    /// Cleared whenever the floor advances. `None` while not blocked.
    pub bar_blocked_since: Option<ServerTimestamp>,
}

/// A position as carried on the wire in a baseline's `covers_up_to`. Separate
/// from [`EventPosition`] so the encoding is explicit integer milliseconds and
/// always parseable independent of the caller's payload schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WirePosition {
    pub modified_ms: i64,
    pub record_id: Guid,
}

impl From<&EventPosition> for WirePosition {
    fn from(pos: &EventPosition) -> Self {
        Self {
            modified_ms: pos.modified.as_millis(),
            record_id: pos.record_id.clone(),
        }
    }
}

impl From<WirePosition> for EventPosition {
    fn from(wire: WirePosition) -> Self {
        EventPosition::new(
            ServerTimestamp::from_millis(wire.modified_ms),
            wire.record_id,
        )
    }
}

/// Split a snapshot into parts that each fit within `max_bytes`, ready to upload
/// as `_0 .. _{N-1}` packages.
///
/// `measure` returns the serialized size of a candidate part. The engine passes
/// the size of the dual-written baseline record (what actually ships); tests can
/// pass a simpler estimate.
///
/// Entities are packed greedily in order, so concatenating the parts back
/// restores the original snapshot. A single entity whose own record already
/// exceeds `max_bytes` can't be split further, so it becomes a part on its own
/// and the caller decides how to handle the oversize record. An empty snapshot
/// still yields one (empty) part, so a baseline is written to advance the floor.
pub fn split_snapshot<F>(
    entities: Vec<SnapshotEntity>,
    max_bytes: usize,
    mut measure: F,
) -> Vec<Vec<SnapshotEntity>>
where
    F: FnMut(&[SnapshotEntity]) -> usize,
{
    if entities.is_empty() {
        return vec![Vec::new()];
    }
    let mut parts: Vec<Vec<SnapshotEntity>> = Vec::new();
    let mut current: Vec<SnapshotEntity> = Vec::new();
    for entity in entities {
        current.push(entity);
        // Adding this entity took a part that already had others over budget, so
        // close that part without it and start a new part holding it.
        if current.len() > 1 && measure(&current) > max_bytes {
            let overflow = current.pop().expect("just pushed an entity");
            parts.push(std::mem::take(&mut current));
            current.push(overflow);
        }
    }
    parts.push(current);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_position_round_trips() {
        let pos = EventPosition::new(ServerTimestamp::from_millis(1234), Guid::new("rec-1"));
        let wire = WirePosition::from(&pos);
        assert_eq!(wire.modified_ms, 1234);
        let back: EventPosition = wire.clone().into();
        assert_eq!(back, pos);

        let json = serde_json::to_string(&wire).unwrap();
        let parsed: WirePosition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, wire);
    }

    fn entity(id: &str) -> SnapshotEntity {
        SnapshotEntity {
            entity: Guid::new(id),
            payload: serde_json::json!({}),
        }
    }

    /// A measure of 10 bytes per entity.
    fn ten_each(part: &[SnapshotEntity]) -> usize {
        part.len() * 10
    }

    fn ids(parts: &[Vec<SnapshotEntity>]) -> Vec<Vec<&str>> {
        parts
            .iter()
            .map(|p| p.iter().map(|e| e.entity.as_str()).collect())
            .collect()
    }

    #[test]
    fn empty_snapshot_yields_one_empty_part() {
        let parts = split_snapshot(vec![], 100, ten_each);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].is_empty());
    }

    #[test]
    fn everything_in_one_part_when_it_fits() {
        let parts = split_snapshot(vec![entity("a"), entity("b")], 100, ten_each);
        assert_eq!(ids(&parts), vec![vec!["a", "b"]]);
    }

    #[test]
    fn splits_when_over_budget() {
        let all = vec![
            entity("a"),
            entity("b"),
            entity("c"),
            entity("d"),
            entity("e"),
        ];
        // Budget of 25 fits two entities (20) but not three (30).
        let parts = split_snapshot(all, 25, ten_each);
        assert_eq!(ids(&parts), vec![vec!["a", "b"], vec!["c", "d"], vec!["e"]]);
    }

    #[test]
    fn oversize_single_entity_gets_its_own_part() {
        // "big" alone measures over budget; it can't be split, so it stands alone.
        let measure = |part: &[SnapshotEntity]| -> usize {
            part.iter()
                .map(|e| if e.entity.as_str() == "big" { 100 } else { 10 })
                .sum()
        };
        let parts = split_snapshot(vec![entity("a"), entity("big"), entity("b")], 25, measure);
        assert_eq!(ids(&parts), vec![vec!["a"], vec!["big"], vec!["b"]]);
    }

    #[test]
    fn parts_concatenate_back_to_the_snapshot() {
        let all: Vec<SnapshotEntity> = ["a", "b", "c", "d", "e", "f", "g"]
            .iter()
            .map(|id| entity(id))
            .collect();
        let original: Vec<String> = all.iter().map(|e| e.entity.to_string()).collect();

        let parts = split_snapshot(all, 25, ten_each);
        let flattened: Vec<String> = parts
            .into_iter()
            .flatten()
            .map(|e| e.entity.to_string())
            .collect();
        assert_eq!(flattened, original);
    }
}
