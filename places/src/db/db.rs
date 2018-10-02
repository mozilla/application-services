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
use unicode_segmentation::UnicodeSegmentation;
use caseless::Caseless;

use api::matcher::{MatchBehavior, split_after_prefix, split_after_host_and_port};

pub const MAX_VARIABLE_NUMBER: usize = 999;

pub struct PlacesDb {
    pub db: Connection,
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
            format!("PRAGMA key = '{}';", sql_support::escape_string_for_pragma(key))
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
