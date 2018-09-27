/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// XXXXXX - This has been cloned from logins-sql/src/db.rs, on Thom's
// wip-sync-sql-store branch, but with login specific code removed.
// We should work out how to split this into a library we can reuse.

use super::schema;
use error::*;
use hash;
use rusqlite::{
    self,
    types::{FromSql, ToSql},
    Connection, Row,
};
use std::path::Path;

use unicode_segmentation::UnicodeSegmentation;
use caseless::Caseless;

use api::matcher::{MatchBehavior, split_after_prefix, split_after_host_and_port};

pub const MAX_VARIABLE_NUMBER: usize = 999;

pub struct PlacesDb {
    pub db: Connection,
}

// In PRAGMA foo='bar', `'bar'` must be a constant string (it cannot be a
// bound parameter), so we need to escape manually. According to
// https://www.sqlite.org/faq.html, the only character that must be escaped is
// the single quote, which is escaped by placing two single quotes in a row.
fn escape_string_for_pragma(s: &str) -> String {
    s.replace("'", "''")
}

fn unicode_normalize(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.chars().nfd().default_case_fold().nfd().collect()
}

impl PlacesDb {
    pub fn with_connection(db: Connection, encryption_key: Option<&str>) -> Result<Self> {
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

        db.execute_batch(&initial_pragmas)?;
        define_functions(&db)?;

        let mut res = Self { db };
        schema::init(&mut res)?;

        Ok(res)
    }

    pub fn open(path: impl AsRef<Path>, encryption_key: Option<&str>) -> Result<Self> {
        Ok(Self::with_connection(Connection::open(path)?, encryption_key)?)
    }

    pub fn open_in_memory(encryption_key: Option<&str>) -> Result<Self> {
        Ok(Self::with_connection(Connection::open_in_memory()?, encryption_key)?)
    }
}

/// This trait exists so that we can use these helpers on rusqlite::{Transaction,Connection}, as well as
/// PlacesDb. Note that you must import ConnectionUtil in order to call thes methods!
pub trait ConnectionUtil {
    fn conn(&self) -> &Connection;

    // TODO: better versions of most exist in logins-sql.
    // need to get on sharing this stuff...

    fn vacuum(&self) -> Result<()> {
        self.execute("VACUUM")?;
        Ok(())
    }

    fn execute_all(&self, stmts: &[&str]) -> Result<()> {
        for sql in stmts {
            self.execute(sql)?;
        }
        Ok(())
    }

    fn execute(&self, stmt: &str) -> Result<usize> {
        Ok(self.do_exec(stmt, &[], false)?)
    }

    fn execute_cached(&self, stmt: &str) -> Result<usize> {
        Ok(self.do_exec(stmt, &[], true)?)
    }

    fn execute_with_args(&self, stmt: &str, params: &[&ToSql]) -> Result<usize> {
        Ok(self.do_exec(stmt, params, false)?)
    }

    fn execute_cached_with_args(&self, stmt: &str, params: &[&ToSql]) -> Result<usize> {
        Ok(self.do_exec(stmt, params, true)?)
    }

    fn do_exec(&self, sql: &str, params: &[&ToSql], cache: bool) -> Result<usize> {
        let res = if cache {
            self.conn().prepare_cached(sql)
                   .and_then(|mut s| s.execute(params))
        } else {
            self.conn().execute(sql, params)
        };
        if let Err(e) = &res {
            warn!("Error running SQL {}. Statement: {:?}", e, sql);
        }
        Ok(res?)
    }

    fn execute_named(&self, stmt: &str, params: &[(&str, &ToSql)]) -> Result<usize> {
        Ok(self.do_exec_named(stmt, params, false)?)
    }

    fn execute_named_cached(&self, stmt: &str, params: &[(&str, &ToSql)]) -> Result<usize> {
        Ok(self.do_exec_named(stmt, params, true)?)
    }

    fn do_exec_named(&self, sql: &str, params: &[(&str, &ToSql)], cache: bool) -> Result<usize> {
        let res = if cache {
            self.conn().prepare_cached(sql)
                       .and_then(|mut s| s.execute_named(params))
        } else {
            self.conn().execute_named(sql, params)
        };
        if let Err(e) = &res {
            warn!("Error running SQL {}. Statement: {:?}", e, sql);
        }
        Ok(res?)
    }

    fn query_one<T: FromSql>(&self, sql: &str) -> Result<T> {
        let res: T = self.conn().query_row(sql, &[], |row| row.get(0))?;
        Ok(res)
    }

    // Note that there are several differences between these and `self.db.query_row`: it returns
    // None and not an error if no rows are returned, it allows the function to return a result, etc
    fn query_row_cached<T>(&self, sql: &str, args: &[&ToSql], f: impl FnOnce(&Row) -> Result<T>) -> Result<Option<T>> {
        let mut stmt = self.conn().prepare_cached(sql)?;
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
    fn query_row<T>(&self, sql: &str, args: &[&ToSql], f: impl FnOnce(&Row) -> Result<T>) -> Result<Option<T>> {
        let mut stmt = self.conn().prepare(sql)?;
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

    fn query_row_named<T>(&self, sql: &str, args: &[(&str, &ToSql)], f: impl FnOnce(&Row) -> Result<T>) -> Result<Option<T>> {
        let mut stmt = self.conn().prepare(sql)?;
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

impl ConnectionUtil for Connection {
    #[inline]
    fn conn(&self) -> &Connection {
        self
    }
}

impl<'conn> ConnectionUtil for rusqlite::Transaction<'conn> {
    #[inline]
    fn conn(&self) -> &Connection {
        use std::ops::Deref;
        Deref::deref(self)
    }
}

impl ConnectionUtil for PlacesDb {
    #[inline]
    fn conn(&self) -> &Connection {
        &self.db
    }
}

// ----------------------------- end of stuff that should be common --------------------

fn define_functions(c: &Connection) -> Result<()> {
    c.create_scalar_function("get_prefix", 1, true, move |ctx| {
        let href = ctx.get::<String>(0)?;
        let (prefix, _) = split_after_prefix(&href);
        Ok(prefix.to_owned())
    })?;
    c.create_scalar_function("get_host_and_port", 1, true, move |ctx| {
        let href = ctx.get::<String>(0)?;
        let (host_and_port, _) = split_after_host_and_port(&href);
        Ok(host_and_port.to_owned())
    })?;
    c.create_scalar_function("strip_prefix_and_userinfo", 1, true, move |ctx| {
        let href = ctx.get::<String>(0)?;
        let (_, remainder) = split_after_host_and_port(&href);
        Ok(remainder.to_owned())
    })?;
    c.create_scalar_function("reverse_host", 1, true, move |ctx| {
        let host = ctx.get::<String>(0)?;
        // TODO: This should be ASCII in all cases, so we could get a ~10x speedup (
        // according to some microbenchmarks) with something like
        // `let rev_host = String::from_utf8(host.bytes().map(|b| b.to_ascii_lowercase()).collect())?;`
        // (We may want to map_err and say something like "invalid nonascii host passed to reverse_host")
        let rev_host: String = host.chars().rev().flat_map(|c| c.to_lowercase()).collect();
        Ok(rev_host + ".")
    })?;
    c.create_scalar_function("autocomplete_match", -1, true, move |ctx| {
        let search_string = ctx.get::<Option<String>>(0)?.unwrap_or_default();
        let url = ctx.get::<Option<String>>(1)?.unwrap_or_default();
        let title = ctx.get::<Option<String>>(2)?.unwrap_or_default();
        let tags = ctx.get::<Option<String>>(3)?;
        let visit_count = ctx.get::<i64>(4)?;
        let _typed = ctx.get::<bool>(5)?;
        let bookmarked = ctx.get::<bool>(6)?;
        let open_page_count = ctx.get::<Option<i64>>(7)?;
        let _match_behavior = ctx.get::<MatchBehavior>(8);

        if !(visit_count > 0
            || bookmarked
            || tags.is_some()
            || open_page_count.map(|count| count > 0).unwrap_or(false))
        {
            return Ok(false);
        }

        let trimmed_url = &url[..255.min(url.len())];
        let trimmed_title = &title[..255.min(title.len())];

        let norm_url = unicode_normalize(trimmed_url);
        let norm_title = unicode_normalize(trimmed_title);
        let norm_search = unicode_normalize(&search_string);
        let norm_tags = unicode_normalize(&tags.unwrap_or_default());
        let every_token_matched = norm_search
            .unicode_words()
            .all(|token| norm_url.contains(token) ||
                         norm_tags.contains(token) ||
                         norm_title.contains(token));

        Ok(every_token_matched)
    })?;
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
    Ok(())
}

// Sanity check that we can create a database.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open() {
        PlacesDb::open_in_memory(None).expect("no memory db");
    }

    #[test]
    fn test_reverse_host() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let rev_host: String = conn.db.query_row("SELECT reverse_host('www.mozilla.org')", &[], |row| row.get(0)).unwrap();
        assert_eq!(rev_host, "gro.allizom.www.");

        let rev_host: String = conn.db.query_row("SELECT reverse_host('')", &[], |row| row.get(0)).unwrap();
        assert_eq!(rev_host, ".");
    }
}
