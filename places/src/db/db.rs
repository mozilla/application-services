/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// XXXXXX - This has been cloned from logins-sql/src/db.rs, on Thom's
// wip-sync-sql-store branch, but with login specific code removed.
// We should work out how to split this into a library we can reuse.

use rusqlite::{self, Connection, types::{ToSql, FromSql}, Row};
use error::*;
use std::sync::{Arc, Mutex};
use super::schema;
use hash;
use frecency;

use std::path::Path;

pub const MAX_VARIABLE_NUMBER: usize = 999;

pub struct PlacesDb {
    // Read/Write connection
    pub db: Connection,
}

// In PRAGMA foo='bar', `'bar'` must be a constant string (it cannot be a
// bound parameter), so we need to escape manually. According to
// https://www.sqlite.org/faq.html, the only character that must be escaped is
// the single quote, which is escaped by placing two single quotes in a row.
fn escape_string_for_pragma(s: &str) -> String {
    s.replace("'", "''")
}

impl PlacesDb {
    // This takes two connections, one of which being protected by a mutex because of the annoying
    // requirements rusqlite puts on `create_scalar_function`, which make it very difficult
    // to have a function which runs a query. XXX: Elaborate here.
    pub fn with_connections(write_conn: Connection,
                            read_conn: Arc<Mutex<Connection>>,
                            encryption_key: Option<&str>) -> Result<Self> {
        #[cfg(test)] {
//            util::init_test_logging();
        }

        let encryption_pragmas = if let Some(key) = encryption_key {
            // TODO: We probably should support providing a key that doesn't go
            // through PBKDF2 (e.g. pass it in as hex, or use sqlite3_key
            // directly. See https://www.zetetic.net/sqlcipher/sqlcipher-api/#key
            // "Raw Key Data" example. Note that this would be required to open
            // existing iOS sqlcipher databases).
            format!("PRAGMA key = '{}';", escape_string_for_pragma(key))
        } else {
            "".to_owned()
        };

        // `temp_store = 2` is required on Android to force the DB to keep temp
        // files in memory, since on Android there's no tmp partition. See
        // https://github.com/mozilla/mentat/issues/505. Ideally we'd only
        // do this on Android, or allow caller to configure it.
        let initial_pragmas = format!("
            {}
            PRAGMA temp_store = 2;
        ", encryption_pragmas);

        write_conn.execute_batch(&initial_pragmas)?;
        {
            let read_conn = read_conn.lock().unwrap();
            read_conn.execute_batch(&initial_pragmas)?;
        }
        define_functions(&write_conn, read_conn)?;

        let mut res = Self { db: write_conn };
        schema::init(&mut res)?;

        Ok(res)
    }

    // Note: users wanting an in-memory connection should open with a path that specifies a *named*
    // in-memory database, which isn't possible using rusqlite::Connection::open_in_memory. This is
    // because we need multiple connections open to the DB. For example
    pub fn open(path: impl AsRef<Path>, encryption_key: Option<&str>) -> Result<Self> {
        let p = path.as_ref();
        let write_conn = Connection::open_with_flags(p,
            rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE |
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE |
            rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        )?;

        let read_conn = Arc::new(Mutex::new(Connection::open_with_flags(p,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY |
            rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE
        )?));
        Ok(Self::with_connections(write_conn, read_conn, encryption_key)?)
    }

    pub fn vacuum(&self) -> Result<()> {
        self.execute("VACUUM")?;
        Ok(())
    }

    pub fn execute_all(&self, stmts: &[&str]) -> Result<()> {
        for sql in stmts {
            self.execute(sql)?;
        }
        Ok(())
    }

    #[inline]
    pub fn execute(&self, stmt: &str) -> Result<usize> {
        Ok(self.do_exec(stmt, &[], false)?)
    }

    #[inline]
    pub fn execute_cached(&self, stmt: &str) -> Result<usize> {
        Ok(self.do_exec(stmt, &[], true)?)
    }

    #[inline]
    pub fn execute_with_args(&self, stmt: &str, params: &[&ToSql]) -> Result<usize> {
        Ok(self.do_exec(stmt, params, false)?)
    }

    #[inline]
    pub fn execute_cached_with_args(&self, stmt: &str, params: &[&ToSql]) -> Result<usize> {
        Ok(self.do_exec(stmt, params, true)?)
    }

    fn do_exec(&self, sql: &str, params: &[&ToSql], cache: bool) -> Result<usize> {
        let res = if cache {
            self.db.prepare_cached(sql)
                   .and_then(|mut s| s.execute(params))
        } else {
            self.db.execute(sql, params)
        };
        if let Err(e) = &res {
            warn!("Error running SQL {}. Statement: {:?}", e, sql);
        }
        Ok(res?)
    }

    #[inline]
    pub fn execute_named(&self, stmt: &str, params: &[(&str, &ToSql)]) -> Result<usize> {
        Ok(self.do_exec_named(stmt, params, false)?)
    }

    #[inline]
    pub fn execute_named_cached(&self, stmt: &str, params: &[(&str, &ToSql)]) -> Result<usize> {
        Ok(self.do_exec_named(stmt, params, true)?)
    }

    fn do_exec_named(&self, sql: &str, params: &[(&str, &ToSql)], cache: bool) -> Result<usize> {
        let res = if cache {
            self.db.prepare_cached(sql)
                   .and_then(|mut s| s.execute_named(params))
        } else {
            self.db.execute_named(sql, params)
        };
        if let Err(e) = &res {
            warn!("Error running SQL {}. Statement: {:?}", e, sql);
        }
        Ok(res?)
    }

    pub fn query_one<T: FromSql>(&self, sql: &str) -> Result<T> {
        let res: T = self.db.query_row(sql, &[], |row| row.get(0))?;
        Ok(res)
    }

    // Note that there are several differences between these and `self.db.query_row`: it returns
    // None and not an error if no rows are returned, it allows the function to return a result, etc
    pub fn query_row_cached<T>(&self, sql: &str, args: &[&ToSql], f: impl FnOnce(&Row) -> Result<T>) -> Result<Option<T>> {
        let mut stmt = self.db.prepare_cached(sql)?;
        let res = stmt.query(args);
        if let Err(e) = &res {
            warn!("Error executing query: {}. Query: {}", e, sql);
        }
        let mut rows = res?;
        match rows.next() {
            Some(result) => Ok(Some(f(&result?)?)),
            None => Ok(None),
        }
    }

    // cached and uncached stmt types are completely different so we can't remove the duplication
    // between query_row_cached and query_row... :/
    pub fn query_row<T>(&self, sql: &str, args: &[&ToSql], f: impl FnOnce(&Row) -> Result<T>) -> Result<Option<T>> {
        let mut stmt = self.db.prepare(sql)?;
        let res = stmt.query(args);
        if let Err(e) = &res {
            warn!("Error executing query: {}. Query: {}", e, sql);
        }
        let mut rows = res?;
        match rows.next() {
            Some(result) => Ok(Some(f(&result?)?)),
            None => Ok(None),
        }
    }

    pub fn query_row_named<T>(&self, sql: &str, args: &[(&str, &ToSql)], f: impl FnOnce(&Row) -> Result<T>) -> Result<Option<T>> {
        let mut stmt = self.db.prepare(sql)?;
        let res = stmt.query_named(args);
        if let Err(e) = &res {
            warn!("Error executing query: {}. Query: {}", e, sql);
        }
        let mut rows = res?;
        match rows.next() {
            Some(result) => Ok(Some(f(&result?)?)),
            None => Ok(None),
        }
    }
}

// ----------------------------- end of stuff that should be common --------------------

fn define_functions(write: &Connection, read: Arc<Mutex<Connection>>) -> Result<()> {
    {
        // Scope unlock of `read`
        let read = read.clone();
        let unlocked = read.lock().unwrap();
        for c in &[write, &*unlocked] {
            // Add functions which don't need access to the database connection here.
            c.create_scalar_function("hash", -1, true, move |ctx| {
                Ok(match ctx.len() {
                    1 => {
                        let value = ctx.get::<String>(0)?;
                        hash::hash_url(&value)
                    }
                    2 => {
                        let value = ctx.get::<String>(0)?;
                        let mode = ctx.get::<String>(1)?;
                        match mode.as_str() {
                            "" => hash::hash_url(&value),
                            "prefix_lo" => hash::hash_url_prefix(&value, hash::PrefixMode::Lo),
                            "prefix_hi" => hash::hash_url_prefix(&value, hash::PrefixMode::Hi),
                            arg => {
                                return Err(rusqlite::Error::UserFunctionError(format!(
                                    "`hash` second argument must be either '', 'prefix_lo', or 'prefix_hi', got {:?}.",
                                    arg).into()));
                            }
                        }
                    }
                    n => {
                        return Err(rusqlite::Error::UserFunctionError(format!(
                            "`hash` expects 1 or 2 arguments, got {}.", n).into()));
                    }
                } as i64)
            })?;
        }
    }

    // Functions which do need access to the database connection should go here.
    // Each one will need to do it's own `let read = read.clone();`
    {
        let read = read.clone();
        write.create_scalar_function("compute_frecency", -1, true, move |ctx| {
            let (page_id, is_redirect) = match ctx.len() {
                1 => (ctx.get::<i64>(0)?, None),
                2 => (ctx.get::<i64>(0)?, ctx.get::<Option<bool>>(1)?),
                n => {
                    return Err(rusqlite::Error::UserFunctionError(format!(
                        "`compute_frecency` expects 1 or 2 arguments, got {}.", n).into()));
                }
            };
            let unlocked_read = read.lock().unwrap();
            Ok(frecency::calculate_frecency(
                &unlocked_read,
                &frecency::DEFAULT_FRECENCY_SETTINGS,
                page_id,
                is_redirect
            ).map_err(|e| {
                // XXX error::Error implements Fail (and thus) cannot implement std::error::Error,
                // which is required for `rusqlite::Error::UserFunctionError`. We convert the error
                // to a string which does implement the right type, but we're losing information
                // about the issue. Fixing this might just be writing an `impl std::error::Error`
                // for our error type!
                error!("calulate_frecency hit an error: {:?}", e);
                use failure::Fail;
                rusqlite::Error::UserFunctionError(e.compat().into())
            })?)
        })?;
    }
    Ok(())
}

// Sanity check that we can create a database.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open() {
        PlacesDb::open("file:example_memory_database?mode=memory&cache=shared", None)
            .expect("no memory db");
    }
}
