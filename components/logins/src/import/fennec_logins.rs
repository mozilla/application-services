/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::{engine::PasswordEngine, login::SyncStatus};
use rusqlite::{functions::Context, named_params, Connection};
use sql_support::ConnExt;
use std::{path::Path, time::SystemTime};
use url::Url;

pub fn import(password_engine: &PasswordEngine, path: impl AsRef<std::path::Path>) -> Result<()> {
    let url = ensure_url_path(path)?;
    do_import(password_engine, url)
}

// This import method does not support an android DB locked with a master password.
fn do_import(password_engine: &PasswordEngine, android_db_file_url: Url) -> Result<()> {
    let conn = password_engine.conn();
    conn.create_scalar_function("sanitize_timestamp", 1, true, sanitize_timestamp)?;

    let scope = password_engine.begin_interrupt_scope();

    // Not sure why, but apparently beginning a transaction sometimes
    // fails if we open the DB as read-only. Hopefully we don't
    // unintentionally write to it anywhere...
    // android_db_file_url.query_pairs_mut().append_pair("mode", "ro");

    log::trace!("Attaching database {}", android_db_file_url);
    let auto_detach = attached_database(&conn, &android_db_file_url, "fennec")?;

    let tx = conn.unchecked_transaction()?;

    log::debug!("Inserting the logins");
    conn.execute_batch(&INSERT_LOGINS)?;
    scope.err_if_interrupted()?;

    log::debug!("Committing...");
    tx.commit()?;

    log::info!("Successfully imported logins!");

    auto_detach.execute_now()?;

    Ok(())
}

lazy_static::lazy_static! {
    // Insert logins
    static ref INSERT_LOGINS: String = format!(
        "INSERT OR IGNORE INTO loginsL (
            hostname,
            httpRealm,
            formSubmitURL,
            usernameField,
            passwordField,
            timesUsed,
            username,
            password,
            guid,
            timeCreated,
            timeLastUsed,
            timePasswordChanged,
            local_modified,
            is_deleted,
            sync_status
        )
            SELECT
                l.hostname,
                l.httpRealm,
                l.formSubmitURL,
                l.usernameField,
                l.passwordField,
                IFNULL(l.timesUsed, 0),
                l.encryptedUsername,
                l.encryptedPassword,
                l.guid,
                sanitize_timestamp(l.timeCreated),
                l.timeLastUsed,
                sanitize_timestamp(l.timePasswordChanged),
                NULL,
                0, -- is_deleted
                {new} -- sync_status
            FROM fennec.logins l
            WHERE -- Checks copied from `Login::check_valid`.
            l.hostname != '' AND
            l.encryptedPassword != '' AND
            (
                (l.httpRealm IS NULL AND l.formSubmitURL IS NOT NULL) OR
                (l.httpRealm IS NOT NULL AND l.formSubmitURL IS NULL)
            )
            ",
        new = SyncStatus::New as u8
    );
}

pub fn attached_database<'a>(
    conn: &'a Connection,
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
    conn: &'a Connection,
    sql: String,
}

impl<'a> ExecuteOnDrop<'a> {
    pub fn new(conn: &'a Connection, sql: String) -> Self {
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

/// If `p` is a file URL, return it, otherwise try and make it one.
///
/// Errors if `p` is a relative non-url path, or if it's a URL path
/// that's isn't a `file:` URL.
pub fn ensure_url_path(p: impl AsRef<Path>) -> Result<Url> {
    if let Some(u) = p.as_ref().to_str().and_then(|s| Url::parse(s).ok()) {
        if u.scheme() == "file" {
            Ok(u)
        } else {
            Err(ErrorKind::IllegalDatabasePath(p.as_ref().to_owned()).into())
        }
    } else {
        let p = p.as_ref();
        let u = Url::from_file_path(p).map_err(|_| ErrorKind::IllegalDatabasePath(p.to_owned()))?;
        Ok(u)
    }
}

#[inline(never)]
// Adapted from places::import::common::sql_fns::sanitize_timestamp but works with `i64`.
pub fn sanitize_timestamp(ctx: &Context<'_>) -> rusqlite::Result<i64> {
    let now_ms = crate::util::system_time_ms_i64(SystemTime::now());
    Ok(if let Ok(ts) = ctx.get::<i64>(0) {
        if ts < now_ms {
            ts
        } else {
            now_ms
        }
    } else {
        now_ms
    })
}
