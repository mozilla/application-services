/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::api::places_api::SyncConn;
use crate::error::*;
use rusqlite::named_params;
use url::Url;

pub mod sql_fns {
    use crate::storage::URL_LENGTH_MAX;
    use crate::types::Timestamp;
    use rusqlite::{functions::Context, types::ValueRef, Result};
    use std::convert::TryFrom;
    use url::Url;

    #[inline(never)]
    pub fn sanitize_timestamp(ctx: &Context<'_>) -> Result<Timestamp> {
        let now = Timestamp::now();
        let is_sane = |ts: Timestamp| -> bool { Timestamp::EARLIEST < ts && ts < now };
        if let Ok(ts) = ctx.get::<i64>(0) {
            let ts = Timestamp(u64::try_from(ts).unwrap_or(0));
            if is_sane(ts) {
                return Ok(ts);
            }
            // Maybe the timestamp was actually in μs?
            let ts = Timestamp(ts.as_millis() / 1000);
            if is_sane(ts) {
                return Ok(ts);
            }
        }
        Ok(now)
    }

    #[inline(never)]
    pub fn validate_url(ctx: &Context<'_>) -> Result<Option<String>> {
        let val = ctx.get_raw(0);
        let href = if let ValueRef::Text(s) = val {
            std::str::from_utf8(s)?
        } else {
            return Ok(None);
        };
        if href.len() > URL_LENGTH_MAX {
            return Ok(None);
        }
        if let Ok(url) = Url::parse(href) {
            Ok(Some(url.into_string()))
        } else {
            Ok(None)
        }
    }

    #[inline(never)]
    pub fn is_valid_url(ctx: &Context<'_>) -> Result<Option<bool>> {
        Ok(match ctx.get_raw(0) {
            ValueRef::Text(s) if s.len() <= URL_LENGTH_MAX => {
                if let Ok(s) = std::str::from_utf8(s) {
                    Some(Url::parse(s).is_ok())
                } else {
                    Some(false)
                }
            }
            _ => Some(false),
        })
    }
}

pub fn attached_database<'a>(
    conn: &'a SyncConn<'a>,
    path: &Url,
    db_alias: &'static str,
) -> Result<ExecuteOnDrop<'a>> {
    conn.execute_named(
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
    conn: &'a SyncConn<'a>,
    sql: String,
}

impl<'a> ExecuteOnDrop<'a> {
    pub fn new(conn: &'a SyncConn<'a>, sql: String) -> Self {
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
            log::error!("Failed to clean up after import! {}", e);
            log::debug!("  Failed query: {}", &self.sql);
        }
    }
}
