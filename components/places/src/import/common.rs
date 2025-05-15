/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::PlacesDb;
use crate::error::*;
use rusqlite::{named_params, Connection};
use serde::Serialize;
use sql_support::ConnExt;
use types::Timestamp;
use url::Url;

// sanitize_timestamp can't use `Timestamp::now();` directly because it needs
// to sanitize both created and modified, plus ensure modified isn't before
// created - which isn't possible with the non-monotonic timestamp.
// So we have a static `NOW`, which will be initialized the first time it is
// referenced, and that value subsequently used for every imported bookmark (and
// note that it's only used in cases where the existing timestamps are invalid.)
// This is fine for our use-case, where we do exactly one import as soon as the
// process starts.
lazy_static::lazy_static! {
    pub static ref NOW: Timestamp = Timestamp::now();
}

pub mod sql_fns {
    use crate::import::common::NOW;
    use crate::storage::URL_LENGTH_MAX;
    use rusqlite::{functions::Context, types::ValueRef, Result};
    use types::Timestamp;
    use url::Url;

    fn sanitize_timestamp(ts: i64) -> Result<Timestamp> {
        let now = *NOW;
        let is_sane = |ts: Timestamp| -> bool { Timestamp::EARLIEST <= ts && ts <= now };
        let ts = Timestamp(u64::try_from(ts).unwrap_or(0));
        if is_sane(ts) {
            return Ok(ts);
        }
        // Maybe the timestamp was actually in μs?
        let ts = Timestamp(ts.as_millis() / 1000);
        if is_sane(ts) {
            return Ok(ts);
        }
        Ok(now)
    }

    // Unfortunately dates for history visits in old iOS databases
    // have a type of `REAL` in their schema. This means they are represented
    // as a float value and have to be read as f64s.
    // This is unconventional, and you probably don't need to use
    // this function otherwise.
    #[inline(never)]
    pub fn sanitize_float_timestamp(ctx: &Context<'_>) -> Result<Timestamp> {
        let ts = ctx
            .get::<f64>(0)
            .map(|num| {
                if num.is_normal() && num > 0.0 {
                    num.round() as i64
                } else {
                    0
                }
            })
            .unwrap_or(0);
        sanitize_timestamp(ts)
    }

    #[inline(never)]
    pub fn sanitize_integer_timestamp(ctx: &Context<'_>) -> Result<Timestamp> {
        sanitize_timestamp(ctx.get::<i64>(0).unwrap_or(0))
    }

    // Possibly better named as "normalize URL" - even in non-error cases, the
    // result string may not be the same href used passed as input.
    #[inline(never)]
    pub fn validate_url(ctx: &Context<'_>) -> Result<Option<String>> {
        let val = ctx.get_raw(0);
        let href = if let ValueRef::Text(s) = val {
            String::from_utf8_lossy(s).to_string()
        } else {
            return Ok(None);
        };
        if href.len() > URL_LENGTH_MAX {
            return Ok(None);
        }
        if let Ok(url) = Url::parse(&href) {
            Ok(Some(url.into()))
        } else {
            Ok(None)
        }
    }

    // Sanitize a text column into valid utf-8. Leave NULLs alone, but all other
    // types are converted to an empty string.
    #[inline(never)]
    pub fn sanitize_utf8(ctx: &Context<'_>) -> Result<Option<String>> {
        let val = ctx.get_raw(0);
        Ok(match val {
            ValueRef::Text(s) => Some(String::from_utf8_lossy(s).to_string()),
            ValueRef::Null => None,
            _ => Some("".to_owned()),
        })
    }
}

pub fn attached_database<'a>(
    conn: &'a PlacesDb,
    path: &Url,
    db_alias: &'static str,
) -> Result<ExecuteOnDrop<'a>> {
    conn.execute(
        "ATTACH DATABASE :path AS :db_alias",
        named_params! {
            ":path": path.as_str(),
            ":db_alias": db_alias,
        },
    )?;
    Ok(ExecuteOnDrop {
        conn,
        sql: format!("DETACH DATABASE {};", db_alias),
    })
}

/// We use/abuse the mirror to perform our import, but need to clean it up
/// afterwards. This is an RAII helper to do so.
///
/// Ideally, you should call `execute_now` rather than letting this drop
/// automatically, as we can't report errors beyond logging when running
/// Drop.
pub struct ExecuteOnDrop<'a> {
    conn: &'a PlacesDb,
    sql: String,
}

impl<'a> ExecuteOnDrop<'a> {
    pub fn new(conn: &'a PlacesDb, sql: String) -> Self {
        Self { conn, sql }
    }

    pub fn execute_now(self) -> Result<()> {
        self.conn.execute_batch(&self.sql)?;
        // Don't run our `drop` function.
        std::mem::forget(self);
        Ok(())
    }
}

impl Drop for ExecuteOnDrop<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.conn.execute_batch(&self.sql) {
            error_support::report_error!(
                "places-cleanup-failure",
                "Failed to clean up after import! {}",
                e
            );
            debug!("  Failed query: {}", &self.sql);
        }
    }
}

pub fn select_count(conn: &PlacesDb, stmt: &str) -> Result<u32> {
    let count: Result<Option<u32>> =
        conn.try_query_row(stmt, [], |row| Ok(row.get::<_, u32>(0)?), false);
    count.map(|op| op.unwrap_or(0))
}

#[derive(Serialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct HistoryMigrationResult {
    pub num_total: u32,
    pub num_succeeded: u32,
    pub num_failed: u32,
    pub total_duration: u64,
}

pub fn define_history_migration_functions(c: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    c.create_scalar_function(
        "validate_url",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        crate::import::common::sql_fns::validate_url,
    )?;
    c.create_scalar_function(
        "sanitize_timestamp",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        crate::import::common::sql_fns::sanitize_integer_timestamp,
    )?;
    c.create_scalar_function(
        "hash",
        -1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        crate::db::db::sql_fns::hash,
    )?;
    c.create_scalar_function(
        "generate_guid",
        0,
        FunctionFlags::SQLITE_UTF8,
        crate::db::db::sql_fns::generate_guid,
    )?;
    c.create_scalar_function(
        "sanitize_utf8",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        crate::import::common::sql_fns::sanitize_utf8,
    )?;
    c.create_scalar_function(
        "sanitize_float_timestamp",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        crate::import::common::sql_fns::sanitize_float_timestamp,
    )?;
    Ok(())
}
