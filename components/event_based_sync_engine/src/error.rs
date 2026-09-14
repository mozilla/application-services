/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::schema::SchemaVersion;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors produced by the event sync engine.
///
/// A single error type (like `sync15::Error`); the consuming crate maps these
/// into its own FFI-facing errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("JSON (de)serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// The event body carries no schema representation this client can read
    /// (client is below the event's supported floor).
    #[error("event body carries no schema representation readable at version {reader} (available: {available:?})")]
    NoReadableSchema {
        reader: SchemaVersion,
        available: Vec<SchemaVersion>,
    },

    /// A migration between two adjacent schema versions was never registered.
    #[error("no migration registered for schema step {from} -> {to}")]
    MissingMigration {
        from: SchemaVersion,
        to: SchemaVersion,
    },

    /// The [`crate::MetadataStore`] failed; string supplied by the impl.
    #[error("metadata store error: {0}")]
    Metadata(String),
}
