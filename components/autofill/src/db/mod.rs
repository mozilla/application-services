/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod addresses;
pub mod credit_cards;
pub mod models;
pub mod schema;
pub mod store;

use crate::error::*;

use interrupt_support::{SqlInterruptHandle, SqlInterruptScope};
use rusqlite::{Connection, OpenFlags};
use sql_support::open_database;
use std::sync::Arc;
use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};
use url::Url;

pub struct AutofillDb {
    pub writer: Connection,
    interrupt_handle: Arc<SqlInterruptHandle>,
}

impl AutofillDb {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = normalize_path(db_path)?;
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

fn unurl_path(p: impl AsRef<Path>) -> PathBuf {
    p.as_ref()
        .to_str()
        .and_then(|s| Url::parse(s).ok())
        .and_then(|u| {
            if u.scheme() == "file" {
                u.to_file_path().ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| p.as_ref().to_owned())
}

fn normalize_path(p: impl AsRef<Path>) -> Result<PathBuf> {
    let path = unurl_path(p);
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }
    // It probably doesn't exist yet. This is an error, although it seems to
    // work on some systems.
    //
    // We resolve this by trying to canonicalize the parent directory, and
    // appending the requested file name onto that. If we can't canonicalize
    // the parent, we return an error.
    //
    // Also, we return errors if the path ends in "..", if there is no
    // parent directory, etc.
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::IllegalDatabasePath(path.clone()))?;

    let parent = path
        .parent()
        .ok_or_else(|| Error::IllegalDatabasePath(path.clone()))?;

    let mut canonical = parent.canonicalize()?;
    canonical.push(file_name);
    Ok(canonical)
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
