/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::schema;
use interrupt_support::{SqlInterruptHandle, SqlInterruptScope};
use parking_lot::Mutex;
use rusqlite::types::{FromSql, ToSql};
use rusqlite::Connection;
use rusqlite::OpenFlags;
use sql_support::open_database::open_database_with_flags;
use sql_support::ConnExt;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;

/// The inner database connection state, allowing graceful close handling.
pub enum BreachAlertsDbInner {
    Open(Connection),
    Closed,
}

pub struct BreachAlertsDb {
    pub writer: BreachAlertsDbInner,
    interrupt_handle: Arc<SqlInterruptHandle>,
}

impl BreachAlertsDb {
    /// Create a new, or fetch an already open, BreachAlertsDb backed by a file on disk.
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = normalize_path(db_path)?;
        Self::new_named(db_path)
    }

    /// Create a new, or fetch an already open, memory-based BreachAlertsDb.
    #[cfg(test)]
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

        let conn =
            open_database_with_flags(db_path, flags, &schema::BreachAlertsConnectionInitializer)?;
        Ok(Self {
            interrupt_handle: Arc::new(SqlInterruptHandle::new(&conn)),
            writer: BreachAlertsDbInner::Open(conn),
        })
    }

    pub fn interrupt_handle(&self) -> Arc<SqlInterruptHandle> {
        Arc::clone(&self.interrupt_handle)
    }

    #[allow(dead_code)]
    pub fn begin_interrupt_scope(&self) -> Result<SqlInterruptScope> {
        Ok(self.interrupt_handle.begin_interrupt_scope()?)
    }

    /// Closes the database connection. If there are any unfinalized prepared
    /// statements on the connection, `close` will fail and the connection
    /// will be leaked.
    pub fn close(&mut self) -> Result<()> {
        let conn = match std::mem::replace(&mut self.writer, BreachAlertsDbInner::Closed) {
            BreachAlertsDbInner::Open(conn) => conn,
            BreachAlertsDbInner::Closed => return Ok(()),
        };
        conn.close().map_err(|(_, y)| Error::SqlError(y))
    }

    pub(crate) fn get_connection(&self) -> Result<&Connection> {
        match &self.writer {
            BreachAlertsDbInner::Open(y) => Ok(y),
            BreachAlertsDbInner::Closed => Err(Error::DatabaseConnectionClosed),
        }
    }
}

// We almost exclusively use this ThreadSafeBreachAlertsDb
pub struct ThreadSafeBreachAlertsDb {
    db: Mutex<BreachAlertsDb>,
    interrupt_handle: Arc<SqlInterruptHandle>,
}

impl ThreadSafeBreachAlertsDb {
    pub fn new(db: BreachAlertsDb) -> Self {
        Self {
            interrupt_handle: db.interrupt_handle(),
            db: Mutex::new(db),
        }
    }

    pub fn interrupt_handle(&self) -> Arc<SqlInterruptHandle> {
        Arc::clone(&self.interrupt_handle)
    }

    #[allow(dead_code)]
    pub fn begin_interrupt_scope(&self) -> Result<SqlInterruptScope> {
        Ok(self.interrupt_handle.begin_interrupt_scope()?)
    }
}

// Deref to a Mutex<BreachAlertsDb>, which is how we will use ThreadSafeBreachAlertsDb most of the time
impl Deref for ThreadSafeBreachAlertsDb {
    type Target = Mutex<BreachAlertsDb>;

    #[inline]
    fn deref(&self) -> &Mutex<BreachAlertsDb> {
        &self.db
    }
}

// Also implement AsRef<SqlInterruptHandle> so that we can interrupt this at shutdown
impl AsRef<SqlInterruptHandle> for ThreadSafeBreachAlertsDb {
    fn as_ref(&self) -> &SqlInterruptHandle {
        &self.interrupt_handle
    }
}

pub fn put_meta(db: &Connection, key: &str, value: &dyn ToSql) -> Result<()> {
    db.conn().execute_cached(
        "REPLACE INTO meta (key, value) VALUES (:key, :value)",
        rusqlite::named_params! { ":key": key, ":value": value },
    )?;
    Ok(())
}

pub fn get_meta<T: FromSql>(db: &Connection, key: &str) -> Result<Option<T>> {
    let res = db.conn().try_query_one(
        "SELECT value FROM meta WHERE key = :key",
        &[(":key", &key)],
        true,
    )?;
    Ok(res)
}

pub fn delete_meta(db: &Connection, key: &str) -> Result<()> {
    db.conn()
        .execute_cached("DELETE FROM meta WHERE key = :key", &[(":key", &key)])?;
    Ok(())
}

// Utilities for working with paths.
// (From places_utils - ideally these would be shared, but the use of
// ErrorKind values makes that non-trivial.

/// `Path` is basically just a `str` with no validation, and so in practice it
/// could contain a file URL. Rusqlite takes advantage of this a bit, and says
/// `AsRef<Path>` but really means "anything sqlite can take as an argument".
///
/// Swift loves using file urls (the only support it has for file manipulation
/// is through file urls), so it's handy to support them if possible.
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

/// As best as possible, convert `p` into an absolute path, resolving
/// all symlinks along the way.
///
/// If `p` is a file url, it's converted to a path before this.
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

// Helpers for tests
#[cfg(test)]
pub mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A helper for our tests to get their own memory Api.
    static ATOMIC_COUNTER: AtomicUsize = AtomicUsize::new(0);

    pub fn new_mem_db() -> BreachAlertsDb {
        error_support::init_for_tests();
        let counter = ATOMIC_COUNTER.fetch_add(1, Ordering::Relaxed);
        BreachAlertsDb::new_memory(&format!("test-breach-alerts-{}", counter))
            .expect("should get a db")
    }

    pub fn new_mem_thread_safe_db() -> Arc<ThreadSafeBreachAlertsDb> {
        Arc::new(ThreadSafeBreachAlertsDb::new(new_mem_db()))
    }
}

#[cfg(test)]
mod tests {
    use super::test::*;
    use super::*;

    // Sanity check that we can create a database.
    #[test]
    fn test_open() {
        new_mem_db();
    }

    #[test]
    fn test_meta() -> Result<()> {
        let db = new_mem_db();
        let conn = &db.get_connection()?;
        assert_eq!(get_meta::<String>(conn, "foo")?, None);
        put_meta(conn, "foo", &"bar".to_string())?;
        assert_eq!(get_meta(conn, "foo")?, Some("bar".to_string()));
        delete_meta(conn, "foo")?;
        assert_eq!(get_meta::<String>(conn, "foo")?, None);
        Ok(())
    }
}
