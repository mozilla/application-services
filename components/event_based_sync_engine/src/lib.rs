/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A generic, storage-agnostic, event-based sync engine.
//!
//! Transports opaque, GUID-keyed events instead of reconciling whole data
//! structures. Consumers implement [`reducer::EventReducer`] to integrate
//! winning events; the engine owns the sync logic (ordering, the per-field
//! last-seen gate, schema evolution, baselines). Pure-Rust, like [`sync15`].
//!
//! Modules: [`event`] (event + gate key + total order), [`schema`] (multi-schema
//! payload + write/migrate-on-read), [`reducer`] (storage contract),
//! [`metadata`] (sync bookkeeping persistence; the SQLite impl is in
//! `metadata_sql`, behind the `sql` feature), [`baseline`] (bounding the event
//! stream), [`error`], and [`engine`] (ties them together and implements
//! [`sync15::engine::SyncEngine`]).

#![warn(rust_2018_idioms)]

pub mod baseline;
pub mod engine;
pub mod error;
pub mod event;
pub mod metadata;
#[cfg(feature = "sql")]
pub mod metadata_sql;
pub mod reducer;
pub mod schema;

pub use baseline::{BaselineConfig, BaselineState};
pub use engine::{EventSyncEngine, DEFAULT_MAX_RECORD_PAYLOAD_BYTES};
pub use error::{Error, Result};
pub use event::{
    batch_record_id, EventPosition, GateKey, IncomingEvent, BASELINE_EVENT_TYPE,
    BASELINE_NEEDED_EVENT_TYPE,
};
pub use metadata::{MemoryMetadataStore, MetadataStore};
#[cfg(feature = "sql")]
pub use metadata_sql::SqliteMetadataStore;
pub use reducer::{EventReducer, ResolvedEvent};
pub use schema::{Body, MigrateFn, SchemaRegistry, SchemaVersion};
