/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod addresses;
pub mod credit_cards;
pub mod models;
pub mod passports;
pub mod schema;
pub mod store;

use crate::error::*;

use error_support::error;
use interrupt_support::{SqlInterruptHandle, SqlInterruptScope};
use rusqlite::{Connection, OpenFlags};
use sql_support::open_database;
use sql_support::path::normalize_database_path;
use std::sync::Arc;
use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

pub struct AutofillDb {
    pub writer: Connection,
    interrupt_handle: Arc<SqlInterruptHandle>,
}

impl AutofillDb {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = normalize_database_path(db_path)?;
        Self::new_named(db_path)
    }

    pub fn new_memory(db_path: &str) -> Result<Self> {
        let name = PathBuf::from(format!("file:{}?mode=memory&cache=shared", db_path));
        Self::new_named(name)
    }

    fn new_named(db_path: PathBuf) -> Result<Self> {
        // We always create the read-write connection for an initial open so
        // we can create the schema and/or do version upgrades.
        let flags = OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_READ_WRITE;

        let conn = open_database::open_database_with_flags(
            db_path,
            flags,
            &schema::AutofillConnectionInitializer,
        )?;

        Ok(Self {
            interrupt_handle: Arc::new(SqlInterruptHandle::new(&conn)),
            writer: conn,
        })
    }

    #[inline]
    pub fn begin_interrupt_scope(&self) -> Result<SqlInterruptScope> {
        Ok(self.interrupt_handle.begin_interrupt_scope()?)
    }

    pub fn close(self) {
        if let Err((_, err)) = self.writer.close() {
            // Log the error, but continue with shutdown.
            error!("Failed to close the connection: {:?}", err);
        }
    }
}

impl Deref for AutofillDb {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.writer
    }
}

impl DerefMut for AutofillDb {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.writer
    }
}

/// Runs `op` in a savepoint, rolling back to it if `op` fails, so that a record
/// reported as an error by a bulk function leaves nothing behind. The shared
/// triggers reject a guid that exists in the counterpart table with
/// `RAISE(FAIL)`, which aborts the statement but keeps the row it already
/// inserted - so without this the offending row would be committed along with
/// the rest of the batch, putting the guid in both the data and tombstone
/// tables.
///
/// The outer `Result` is a savepoint failure and aborts the batch; the inner one
/// is the record's own failure.
pub(crate) fn with_savepoint<T>(
    tx: &rusqlite::Transaction<'_>,
    op: impl FnOnce() -> Result<T>,
) -> Result<std::result::Result<T, Error>> {
    tx.execute_batch("SAVEPOINT bulk_record")?;
    match op() {
        Ok(value) => {
            tx.execute_batch("RELEASE bulk_record")?;
            Ok(Ok(value))
        }
        Err(e) => {
            tx.execute_batch("ROLLBACK TO bulk_record; RELEASE bulk_record")?;
            Ok(Err(e))
        }
    }
}

/// Builds a `Timestamp` from millis an application supplied.
///
/// Anything that is not a representable date becomes 0, which already means
/// "unset" for these fields - see `sanitize_timestamp`. A bare `.max(0)` would
/// not be enough: the corrupt values actually seen in the wild arrive *already*
/// huge, because the negative-to-`u64` reinterpretation happened before the
/// value reached us, and one of those would win every "latest wins" comparison
/// in `Metadata::merge`. The tuple constructor is used rather than
/// `Timestamp::from`, which asserts non-zero.
pub(crate) fn timestamp_from_millis(millis: i64) -> types::Timestamp {
    types::Timestamp(types::sanitize_timestamp(millis) as u64)
}

/// How an `update_internal_*` should treat the record's change counter.
pub(crate) enum CounterUpdate {
    /// Record a local change awaiting upload.
    Increment,
    /// Leave the counter alone, for a change that must not be uploaded - eg one
    /// applied by Sync, which is already what the server has.
    Leave,
    /// Replace the counter, for a record whose counter is owned by the caller.
    Set(i64),
}

impl CounterUpdate {
    /// The SQL assigned to `sync_change_counter`, and the value bound to
    /// `:counter` within it. `Leave` adds 0 rather than dropping `:counter` from
    /// the SQL, because rusqlite rejects a named parameter the statement doesn't
    /// use.
    pub(crate) fn as_sql(&self) -> (&'static str, i64) {
        match self {
            Self::Increment => ("sync_change_counter + :counter", 1),
            Self::Leave => ("sync_change_counter + :counter", 0),
            Self::Set(counter) => (":counter", *counter),
        }
    }
}

pub(crate) mod sql_fns {
    use rusqlite::{functions::Context, Result};
    use sync_guid::Guid as SyncGuid;
    use types::Timestamp;

    #[inline(never)]
    #[allow(dead_code)]
    pub fn generate_guid(_ctx: &Context<'_>) -> Result<SyncGuid> {
        Ok(SyncGuid::random())
    }

    #[inline(never)]
    pub fn now(_ctx: &Context<'_>) -> Result<Timestamp> {
        Ok(Timestamp::now())
    }
}

// Helpers for tests
#[cfg(test)]
pub mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A helper for our tests to get their own memory Api.
    static ATOMIC_COUNTER: AtomicUsize = AtomicUsize::new(0);

    pub fn new_mem_db() -> AutofillDb {
        error_support::init_for_tests();
        let counter = ATOMIC_COUNTER.fetch_add(1, Ordering::Relaxed);
        AutofillDb::new_memory(&format!("test_autofill-api-{}", counter))
            .expect("should get an API")
    }
}
