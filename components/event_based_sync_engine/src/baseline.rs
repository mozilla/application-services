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
//! event type [`crate::event::BASELINE_EVENT_TYPE`].
//!
//! The client that writes a baseline also deletes the records it supersedes
//! from the server, once every frame of the baseline is confirmed.

use crate::event::EventPosition;
use crate::schema::SchemaVersion;
use sync15::ServerTimestamp;

/// if no baseline lands within this time (~30 days in ms), the highest-schema client
/// still active takes over.
pub const DEFAULT_BASELINE_TIMEOUT_MS: i64 = 30 * 24 * 60 * 60 * 1000;

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
}

impl Default for BaselineConfig {
    fn default() -> Self {
        // Count-based cadence: a baseline roughly every 500 events.
        Self {
            events_per_baseline: Some(500),
            baseline_timeout_ms: Some(DEFAULT_BASELINE_TIMEOUT_MS),
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
    /// Cover of a baseline this client wrote whose superseded records are not yet
    /// deleted from the server. Persisted so an interrupted delete is retried.
    pub delete_up_to: Option<EventPosition>,
}
