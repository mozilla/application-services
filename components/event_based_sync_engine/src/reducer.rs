/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! The reducer: the contract between the engine (which owns sync) and storage.
//!
//! The engine handles all event logic (the last-seen gate, baselines, schema
//! resolution) and hands the reducer only gate-winning events, already resolved
//! to the client's current schema.

use sync15::Guid;

/// An event that won its gate and was resolved to the current schema, ready to
/// apply to local storage.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedEvent {
    /// Caller-defined event type, e.g. `"created"`, `"modify-name"`.
    pub event_type: String,
    /// The entity the event is about.
    pub entity: Guid,
    /// Payload, already migrated to the current schema.
    pub payload: serde_json::Value,
}

/// Implemented by the storage layer. All methods are called by the engine; the
/// reducer never talks to the sync server directly.
///
/// Implementations must guard against feedback loops: applying an event here
/// must not record a new local event back into the engine.
pub trait EventReducer: Send + Sync {
    /// Return the full current state as one opaque payload, for the engine to
    /// serialize into a baseline. The engine never inspects it; it writes it in
    /// every schema, and byte-slices the record if it is too large for one upload.
    ///
    /// Must be deterministic (e.g. sort collections), so the same state produces
    /// the same bytes on every client.
    fn baseline(&self) -> serde_json::Value;

    /// Rebuild (sync wins): replace local state with a baseline payload. It is
    /// the same value a peer returned from [`EventReducer::baseline`], resolved to
    /// this client's schema. Called on a cold start and when this client fell
    /// behind a baseline; the engine then re-applies the client's own queued
    /// events through [`EventReducer::apply_event`].
    fn apply_baseline(&self, payload: serde_json::Value);

    /// Apply a single winning event.
    ///
    /// Event order across clients isn't guaranteed, so the target entity may
    /// already be gone (e.g. `modify-name` after `deleted`); tolerate that.
    fn apply_event(&self, event: ResolvedEvent);

    /// Erase all local data for this collection. Called from the engine's
    /// `wipe`, e.g. when the user disconnects on desktop. Must not record local
    /// events: a wipe is local only, never synced as deletes.
    fn wipe(&self);
}
