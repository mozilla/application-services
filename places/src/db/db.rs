/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// XXXXXX - This has been cloned from logins-sql/src/db.rs, on Thom's
// wip-sync-sql-store branch, but with login specific code removed.
// We should work out how to split this into a library we can reuse.

use super::schema;
use error::*;
use hash;
use rusqlite::{self, Connection};
use sql_support::{self, ConnExt};
use std::path::Path;
use std::ops::Deref;
use util;

use api::matcher::{MatchBehavior, split_after_prefix, split_after_host_and_port};

pub const MAX_VARIABLE_NUMBER: usize = 999;

pub struct PlacesDb {
    pub db: Connection,
}

impl PlacesDb {
    pub fn with_connection(db: Connection, encryption_key: Option<&str>) -> Result<Self> {
        const PAGE_SIZE: u32 = 32768;

        // `encryption_pragmas` is both for `PRAGMA key` and for `PRAGMA page_size` / `PRAGMA
        // cipher_page_size` (Even though nominally page_size has nothing to do with encryption, we
        // need to set `PRAGMA cipher_page_size` for encrypted databases, and `PRAGMA page_size` for
        // unencrypted ones).
        //
        // Note: Unfortunately, for an encrypted database, the page size can not be changed without
        // requiring a data migration, so this must be done somewhat carefully. This restriction
        // *only* exists for encrypted DBs, and unencrypted ones (even unencrypted databases using
        // sqlcipher), don't have this limitation.
        //
        // The value we use (`PAGE_SIZE`) was taken from Desktop Firefox, and seems necessary to
        // help ensure good performance on autocomplete-style queries. The default value is 1024,
        // which the SQLcipher docs themselves say is too small and should be changed.
        let encryption_pragmas = if let Some(key) = encryption_key {
            format!("
                PRAGMA key = '{key}';
                PRAGMA cipher_page_size = {page_size};
            ",
                key = sql_support::escape_string_for_pragma(key),
                page_size = PAGE_SIZE,
            )
        } else {
            format!("PRAGMA page_size = {};", PAGE_SIZE)
        };

        let initial_pragmas = format!("
            {}

            -- `temp_store = 2` is required on Android to force the DB to keep temp
            -- files in memory, since on Android there's no tmp partition. See
            -- https://github.com/mozilla/mentat/issues/505. Ideally we'd only
            -- do this on Android, and/or allow caller to configure it.
            PRAGMA temp_store = 2;

            -- 6MiB, same as the value used for `promiseLargeCacheDBConnection` in PlacesUtils,
            -- which is used to improve query performance for autocomplete-style queries (by
            -- UnifiedComplete). Note that SQLite uses a negative value for this pragma to indicate
            -- that it's in units of KiB.
            PRAGMA cache_size = -6144;
        ",
            encryption_pragmas,
        );

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

impl Drop for PlacesDb {
    fn drop(&mut self) {
        // In line with both the recommendations from SQLite and the behavior of places in
        // Database.cpp, we run `PRAGMA optimize` before closing the connection.
        self.db
            .execute_batch("PRAGMA optimize(0x02);")
            .expect("PRAGMA optimize should always succeed!");
    }
}

impl ConnExt for PlacesDb {
    #[inline]
    fn conn(&self) -> &Connection {
        &self.db
    }
}

impl Deref for PlacesDb {
    type Target = Connection;
    #[inline]
    fn deref(&self) -> &Connection {
        &self.db
    }
}

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
        let mut host = ctx.get::<String>(0)?;
        debug_assert!(host.is_ascii(), "Hosts must be Punycoded");

        host.make_ascii_lowercase();
        let mut rev_host_bytes = host.into_bytes();
        rev_host_bytes.reverse();
        rev_host_bytes.push(b'.');

        let rev_host = String::from_utf8(rev_host_bytes).map_err(|err|
            rusqlite::Error::UserFunctionError(err.into())
        )?;
        Ok(rev_host)
    })?;
    c.create_scalar_function("autocomplete_match", 9, true, move |ctx| {
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

        // Note: URLs are serialized as ASCII. Ideally this would actually to do something
        // equivalent to NS_UnescapeURL, and then normalize that, but for now this is fine.
        let trimmed_url = util::slice_up_to(&url, 255);
        let trimmed_title = util::slice_up_to(&title, 255);

        let norm_title = util::unicode_normalize(trimmed_title);
        let norm_tags = tags.map(|s| util::unicode_normalize(&s)).unwrap_or_default();

        // Note: we know that the search string is the output of `util::to_normalized_words`, so
        // we don't need to unicode_normalize, and can just split on space.
        let every_token_matched = search_string
            .split(' ')
            .all(|token| trimmed_url.contains(token) ||
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
