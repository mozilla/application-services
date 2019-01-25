/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// XXXXXX - This has been cloned from logins/src/db.rs, on Thom's
// wip-sync-sql-store branch, but with login specific code removed.
// We should work out how to split this into a library we can reuse.

use super::schema;
use crate::error::*;
use rusqlite::Connection;
use sql_support::{self, ConnExt};
use std::ops::Deref;
use std::path::Path;

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
            format!(
                "PRAGMA key = '{key}';
                 PRAGMA cipher_page_size = {page_size};",
                key = sql_support::escape_string_for_pragma(key),
                page_size = PAGE_SIZE,
            )
        } else {
            format!(
                "PRAGMA page_size = {};
                 -- Disable calling mlock/munlock for every malloc/free.
                 -- In practice this results in a massive speedup, especially
                 -- for insert-heavy workloads.
                 PRAGMA cipher_memory_security = false;",
                PAGE_SIZE
            )
        };

        let initial_pragmas = format!(
            "
            {}

            -- `temp_store = 2` is required on Android to force the DB to keep temp
            -- files in memory, since on Android there's no tmp partition. See
            -- https://github.com/mozilla/mentat/issues/505. Ideally we'd only
            -- do this on Android, and/or allow caller to configure it.
            -- (although see also bug 1313021, where Firefox enabled it for both
            -- Android and 64bit desktop builds)
            PRAGMA temp_store = 2;

            -- 6MiB, same as the value used for `promiseLargeCacheDBConnection` in PlacesUtils,
            -- which is used to improve query performance for autocomplete-style queries (by
            -- UnifiedComplete). Note that SQLite uses a negative value for this pragma to indicate
            -- that it's in units of KiB.
            PRAGMA cache_size = -6144;

            -- we unconditionally want write-ahead-logging mode
            PRAGMA journal_mode=WAL;
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
        Ok(Self::with_connection(
            Connection::open(path)?,
            encryption_key,
        )?)
    }

    pub fn open_in_memory(encryption_key: Option<&str>) -> Result<Self> {
        Ok(Self::with_connection(
            Connection::open_in_memory()?,
            encryption_key,
        )?)
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
    c.create_scalar_function("get_prefix", 1, true, sql_fns::get_prefix)?;
    c.create_scalar_function("get_host_and_port", 1, true, sql_fns::get_host_and_port)?;
    c.create_scalar_function(
        "strip_prefix_and_userinfo",
        1,
        true,
        sql_fns::strip_prefix_and_userinfo,
    )?;
    c.create_scalar_function("reverse_host", 1, true, sql_fns::reverse_host)?;
    c.create_scalar_function("autocomplete_match", 10, true, sql_fns::autocomplete_match)?;
    c.create_scalar_function("hash", -1, true, sql_fns::hash)?;
    Ok(())
}

mod sql_fns {
    use crate::api::matcher::{split_after_host_and_port, split_after_prefix};
    use crate::hash;
    use crate::match_impl::{AutocompleteMatch, MatchBehavior, SearchBehavior};
    use rusqlite::{functions::Context, types::ValueRef, Error, Result};

    // Helpers for define_functions
    fn get_raw_str<'a>(ctx: &'a Context, fname: &'static str, idx: usize) -> Result<&'a str> {
        ctx.get_raw(idx).as_str().map_err(|e| {
            Error::UserFunctionError(format!("Bad arg {} to '{}': {}", idx, fname, e).into())
        })
    }

    fn get_raw_opt_str<'a>(
        ctx: &'a Context,
        fname: &'static str,
        idx: usize,
    ) -> Result<Option<&'a str>> {
        let raw = ctx.get_raw(idx);
        if raw == ValueRef::Null {
            return Ok(None);
        }
        Ok(Some(raw.as_str().map_err(|e| {
            Error::UserFunctionError(format!("Bad arg {} to '{}': {}", idx, fname, e).into())
        })?))
    }

    // Note: The compiler can't meaningfully inline these, but if we don't put
    // #[inline(never)] on them they get "inlined" into a temporary Box<FnMut>,
    // which doesn't have a name (and itself doesn't get inlined). Adding
    // #[inline(never)] ensures they show up in profiles.

    #[inline(never)]
    pub fn hash(ctx: &Context) -> rusqlite::Result<i64> {
        Ok(match ctx.len() {
            1 => {
                let value = get_raw_str(ctx, "hash", 0)?;
                hash::hash_url(value)
            }
            2 => {
                let value = get_raw_str(ctx, "hash", 0)?;
                let mode = get_raw_str(ctx, "hash", 1)?;
                match mode {
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
                return Err(rusqlite::Error::UserFunctionError(
                    format!("`hash` expects 1 or 2 arguments, got {}.", n).into(),
                ));
            }
        } as i64)
    }

    #[inline(never)]
    pub fn autocomplete_match(ctx: &Context) -> Result<bool> {
        let search_str = get_raw_str(ctx, "autocomplete_match", 0)?;
        let url_str = get_raw_str(ctx, "autocomplete_match", 1)?;
        let title_str = get_raw_opt_str(ctx, "autocomplete_match", 2)?.unwrap_or_default();
        let tags = get_raw_opt_str(ctx, "autocomplete_match", 3)?.unwrap_or_default();
        let visit_count = ctx.get::<u32>(4)?;
        let typed = ctx.get::<bool>(5)?;
        let bookmarked = ctx.get::<bool>(6)?;
        let open_page_count = ctx.get::<Option<u32>>(7)?.unwrap_or(0);
        let match_behavior = ctx.get::<MatchBehavior>(8)?;
        let search_behavior = ctx.get::<SearchBehavior>(9)?;

        let matcher = AutocompleteMatch {
            search_str,
            url_str,
            title_str,
            tags,
            visit_count,
            typed,
            bookmarked,
            open_page_count,
            match_behavior,
            search_behavior,
        };
        Ok(matcher.invoke())
    }

    #[inline(never)]
    pub fn reverse_host(ctx: &Context) -> Result<String> {
        // We reuse this memory so no need for get_raw.
        let mut host = ctx.get::<String>(0)?;
        debug_assert!(host.is_ascii(), "Hosts must be Punycoded");

        host.make_ascii_lowercase();
        let mut rev_host_bytes = host.into_bytes();
        rev_host_bytes.reverse();
        rev_host_bytes.push(b'.');

        let rev_host = String::from_utf8(rev_host_bytes).map_err(|_err| {
            rusqlite::Error::UserFunctionError("non-punycode host provided to reverse_host!".into())
        })?;
        Ok(rev_host)
    }

    #[inline(never)]
    pub fn get_prefix(ctx: &Context) -> Result<String> {
        let href = get_raw_str(ctx, "get_prefix", 0)?;
        let (prefix, _) = split_after_prefix(&href);
        Ok(prefix.to_owned())
    }

    #[inline(never)]
    pub fn get_host_and_port(ctx: &Context) -> Result<String> {
        let href = get_raw_str(ctx, "get_host_and_port", 0)?;
        let (host_and_port, _) = split_after_host_and_port(&href);
        Ok(host_and_port.to_owned())
    }

    #[inline(never)]
    pub fn strip_prefix_and_userinfo(ctx: &Context) -> Result<String> {
        let href = get_raw_str(ctx, "strip_prefix_and_userinfo", 0)?;
        let (host_and_port, remainder) = split_after_host_and_port(&href);
        let mut res = String::with_capacity(host_and_port.len() + remainder.len() + 1);
        res += host_and_port;
        res += remainder;
        Ok(res)
    }
}

// Sanity check that we can create a database.
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::NO_PARAMS;

    #[test]
    fn test_open() {
        PlacesDb::open_in_memory(None).expect("no memory db");
    }

    #[test]
    fn test_reverse_host() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let rev_host: String = conn
            .db
            .query_row("SELECT reverse_host('www.mozilla.org')", NO_PARAMS, |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rev_host, "gro.allizom.www.");

        let rev_host: String = conn
            .db
            .query_row("SELECT reverse_host('')", NO_PARAMS, |row| row.get(0))
            .unwrap();
        assert_eq!(rev_host, ".");
    }
}
