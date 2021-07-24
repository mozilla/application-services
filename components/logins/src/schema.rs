/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Logins Schema v4
//! ================
//!
//! The schema we use is a evolution of the firefox-ios logins database format.
//! There are three tables:
//!
//! - `loginsL`: The local table.
//! - `loginsM`: The mirror table.
//! - `loginsSyncMeta`: The table used to to store various sync metadata.
//!
//! ## `loginsL`
//!
//! This stores local login information, also known as the "overlay".
//!
//! `loginsL` is essentially unchanged from firefox-ios, however note the
//! semantic change v4 makes to timestamp fields (which is explained in more
//! detail in the [COMMON_COLS] documentation).
//!
//! It is important to note that `loginsL` is not guaranteed to be present for
//! all records. Synced records may only exist in `loginsM` (although this is
//! not guaranteed). In either case, queries should read from both `loginsL` and
//! `loginsM`.
//!
//! ### `loginsL` Columns
//!
//! Contains all fields in [COMMON_COLS], as well as the following additional
//! columns:
//!
//! - `local_modified`: A millisecond local timestamp indicating when the record
//!   was changed locally, or NULL if the record has never been changed locally.
//!
//! - `is_deleted`: A boolean indicating whether or not this record is a
//!   tombstone.
//!
//! - `sync_status`: A `SyncStatus` enum value, one of
//!
//!     - `0` (`SyncStatus::Synced`): Indicating that the record has been synced
//!
//!     - `1` (`SyncStatus::Changed`): Indicating that the record should be
//!       has changed locally and is known to exist on the server.
//!
//!     - `2` (`SyncStatus::New`): Indicating that the record has never been
//!       synced, or we have been reset since the last time it synced.
//!
//! ## `loginsM`
//!
//! This stores server-side login information, also known as the "mirror".
//!
//! Like `loginsL`, `loginM` has not changed from firefox-ios, beyond the
//! change to store timestamps as milliseconds explained in [COMMON_COLS].
//!
//! Also like `loginsL`, `loginsM` is not guaranteed to have rows for all
//! records. It should not have rows for records which were not synced!
//!
//! It is important to note that `loginsL` is not guaranteed to be present for
//! all records. Synced records may only exist in `loginsM`! Queries should
//! test against both!
//!
//! ### `loginsM` Columns
//!
//! Contains all fields in [COMMON_COLS], as well as the following additional
//! columns:
//!
//! - `server_modified`: the most recent server-modification timestamp
//!   ([sync15::ServerTimestamp]) we've seen for this record. Stored as
//!   a millisecond value.
//!
//! - `is_overridden`: A boolean indicating whether or not the mirror contents
//!   are invalid, and that we should defer to the data stored in `loginsL`.
//!
//! ## `loginsSyncMeta`
//!
//! This is a simple key-value table based on the `moz_meta` table in places.
//! This table was added (by this rust crate) in version 4, and so is not
//! present in firefox-ios.
//!
//! Currently it is used to store two items:
//!
//! 1. The last sync timestamp is stored under [LAST_SYNC_META_KEY], a
//!    `sync15::ServerTimestamp` stored in integer milliseconds.
//!
//! 2. The persisted sync state machine information is stored under
//!    [GLOBAL_STATE_META_KEY]. This is a `sync15::GlobalState` stored as
//!    JSON.
//!

use crate::db::MigrationMetrics;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use lazy_static::lazy_static;
use rusqlite::{Connection, NO_PARAMS};
use sql_support::ConnExt;
use std::time::Instant;

/// The current schema version is 1.  We reset it after the SQLCipher -> plaintext migration.
const VERSION: i64 = 1;
/// Version where we switched from sqlcipher to a plaintext database.
const SQLCIPHER_SWITCHOVER_VERSION: i64 = 5;

/// Every column shared by both tables except for `id`
///
/// Note: `timeCreated`, `timeLastUsed`, and `timePasswordChanged` are in
/// milliseconds. This is in line with how the server and Desktop handle it, but
/// counter to how firefox-ios handles it (hence needing to fix them up
/// firefox-ios on schema upgrade from 3, the last firefox-ios password schema
/// version).
///
/// The reason for breaking from how firefox-ios does things is just because it
/// complicates the code to have multiple kinds of timestamps, for very little
/// benefit. It also makes it unclear what's stored on the server, leading to
/// further confusion.
///
/// However, note that the `local_modified` (of `loginsL`) and `server_modified`
/// (of `loginsM`) are stored as milliseconds as well both on firefox-ios and
/// here (and so they do not need to be updated with the `timeLastUsed`/
/// `timePasswordChanged`/`timeCreated` timestamps.
pub const COMMON_COLS: &str = "
    guid,
    encFields,
    origin,
    httpRealm,
    formActionOrigin,
    usernameField,
    passwordField,
    timeCreated,
    timeLastUsed,
    timePasswordChanged,
    timesUsed
";

const COMMON_SQL: &str = "
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    origin              TEXT NOT NULL,
    -- Exactly one of httpRealm or formActionOrigin should be set
    httpRealm           TEXT,
    formActionOrigin    TEXT,
    usernameField       TEXT,
    passwordField       TEXT,
    timesUsed           INTEGER NOT NULL DEFAULT 0,
    timeCreated         INTEGER NOT NULL,
    timeLastUsed        INTEGER,
    timePasswordChanged INTEGER NOT NULL,
    encFields     TEXT,
    guid                TEXT NOT NULL UNIQUE
";

lazy_static! {
    static ref CREATE_LOCAL_TABLE_SQL: String = format!(
        "CREATE TABLE IF NOT EXISTS loginsL (
            {common_sql},
            -- Milliseconds, or NULL if never modified locally.
            local_modified INTEGER,

            is_deleted     TINYINT NOT NULL DEFAULT 0,
            sync_status    TINYINT NOT NULL DEFAULT 0
        )",
        common_sql = COMMON_SQL
    );
    static ref CREATE_MIRROR_TABLE_SQL: String = format!(
        "CREATE TABLE IF NOT EXISTS loginsM (
            {common_sql},
            -- Milliseconds (a sync15::ServerTimestamp multiplied by
            -- 1000 and truncated)
            server_modified INTEGER NOT NULL,
            is_overridden   TINYINT NOT NULL DEFAULT 0
        )",
        common_sql = COMMON_SQL
    );
    static ref SET_VERSION_SQL: String =
        format!("PRAGMA user_version = {version}", version = VERSION);
}

const CREATE_META_TABLE_SQL: &str = "
    CREATE TABLE IF NOT EXISTS loginsSyncMeta (
        key TEXT PRIMARY KEY,
        value NOT NULL
    )
";

const CREATE_OVERRIDE_HOSTNAME_INDEX_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_loginsM_is_overridden_hostname
    ON loginsM (is_overridden, hostname)
";

const CREATE_DELETED_HOSTNAME_INDEX_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_loginsL_is_deleted_hostname
    ON loginsL (is_deleted, hostname)
";

const CREATE_OVERRIDE_ORIGIN_INDEX_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_loginsM_is_overridden_origin
    ON loginsM (is_overridden, origin)
";

const CREATE_DELETED_ORIGIN_INDEX_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_loginsL_is_deleted_origin
    ON loginsL (is_deleted, origin)
";

// As noted above, we use these when updating from schema v3 (firefox-ios's
// last schema) to convert from microsecond timestamps to milliseconds.
const UPDATE_LOCAL_TIMESTAMPS_TO_MILLIS_SQL: &str = "
    UPDATE loginsL
    SET timeCreated = timeCreated / 1000,
        timeLastUsed = timeLastUsed / 1000,
        timePasswordChanged = timePasswordChanged / 1000
";

const UPDATE_MIRROR_TIMESTAMPS_TO_MILLIS_SQL: &str = "
    UPDATE loginsM
    SET timeCreated = timeCreated / 1000,
        timeLastUsed = timeLastUsed / 1000,
        timePasswordChanged = timePasswordChanged / 1000
";

const RENAME_LOCAL_USERNAME: &str = "
    ALTER TABLE loginsL RENAME username to usernameEnc
";

const RENAME_LOCAL_PASSWORD: &str = "
    ALTER TABLE loginsL RENAME password to passwordEnc
";

const RENAME_MIRROR_USERNAME: &str = "
    ALTER TABLE loginsM RENAME username to usernameEnc
";

const RENAME_MIRROR_PASSWORD: &str = "
    ALTER TABLE loginsM RENAME password to passwordEnc
";

const RENAME_LOCAL_HOSTNAME: &str = "
    ALTER TABLE loginsL RENAME COLUMN hostname TO origin
";

const RENAME_LOCAL_SUBMIT_URL: &str = "
    ALTER TABLE loginsL RENAME COLUMN formSubmitURL TO formActionOrigin
";

const RENAME_MIRROR_HOSTNAME: &str = "
    ALTER TABLE loginsM RENAME COLUMN hostname TO origin
";

const RENAME_MIRROR_SUBMIT_URL: &str = "
    ALTER TABLE loginsM RENAME COLUMN formSubmitURL TO formActionOrigin
";

pub(crate) static LAST_SYNC_META_KEY: &str = "last_sync_time";
pub(crate) static GLOBAL_STATE_META_KEY: &str = "global_state_v2";
pub(crate) static GLOBAL_SYNCID_META_KEY: &str = "global_sync_id";
pub(crate) static COLLECTION_SYNCID_META_KEY: &str = "passwords_sync_id";

pub(crate) fn init(db: &Connection) -> Result<()> {
    let user_version = db.query_one::<i64>("PRAGMA user_version")?;
    log::warn!("user_version: {}", user_version);
    if user_version == 0 {
        return create(db);
    }
    if user_version != VERSION {
        if user_version < VERSION {
            upgrade(db, user_version)?;
        } else {
            log::warn!(
                "Loaded future schema version {} (we only understand version {}). \
                 Optimistically ",
                user_version,
                VERSION
            )
        }
    }
    Ok(())
}

// Allow the redundant Ok() here.  It will make more sense once we have an actual upgrade function.
#[allow(clippy::unnecessary_wraps)]
fn upgrade(_db: &Connection, from: i64) -> Result<()> {
    log::debug!("Upgrading schema from {} to {}", from, VERSION);
    if from == VERSION {
        return Ok(());
    }
    assert_ne!(
        from, 0,
        "Upgrading from user_version = 0 should already be handled (in `init`)"
    );

    // Schema upgrades that should happen after the sqlcipher -> plaintext migration go here
    Ok(())
}

pub(crate) fn create(db: &Connection) -> Result<()> {
    log::debug!("Creating schema");
    db.execute_all(&[
        &*CREATE_LOCAL_TABLE_SQL,
        &*CREATE_MIRROR_TABLE_SQL,
        CREATE_OVERRIDE_ORIGIN_INDEX_SQL,
        CREATE_DELETED_ORIGIN_INDEX_SQL,
        CREATE_META_TABLE_SQL,
        &*SET_VERSION_SQL,
    ])?;
    Ok(())
}

// Run schema upgrades for a SQLCipher database.  This will bring the database up to version 5
// which is required before migrating it to a plaintext database.
pub fn upgrade_sqlcipher_db(db: &mut Connection, encryption_key: &str) -> Result<MigrationMetrics> {
    let user_version = db.query_one::<i64>("PRAGMA user_version")?;

    if user_version == 0 {
        // This logic is largely taken from firefox-ios. AFAICT at some point
        // they went from having schema versions tracked using a table named
        // `tableList` to using `PRAGMA user_version`. This leads to the
        // following logic:
        //
        // - If `tableList` exists, we're hopelessly far in the past
        //
        // - If `tableList` doesn't exist and `PRAGMA user_version` is 0, it's
        //   the first time through
        //
        // In either case, we're not going to be able to migrate any data.  Return an error which
        // signals to the calling code that we can't migrate and therefore we should just delete
        // the sqlcipher db and start fresh.
        return Err(ErrorKind::InvalidDatabaseFile(
            "can't migrate to plaintext when user_version is 0".into(),
        )
        .into());
    }
    log::debug!(
        "Upgrading schema from {} to {} to prep for plaintext migration",
        user_version,
        SQLCIPHER_SWITCHOVER_VERSION
    );
    if user_version >= SQLCIPHER_SWITCHOVER_VERSION {
        // This is a weird case that shouldn't happen in practice, since as soon as we upgrade the
        // schema we immediately export it to plaintext then delete the sqlcipher file.  But if we
        // somehow get here, it seems reasonable to try to continue on with the process, in theory
        // we should be able to export to plaintext.
        return Ok(MigrationMetrics::default());
    }
    let start_time = Instant::now();
    let mut num_processed = 0;
    let mut num_succeeded = 0;
    let mut num_failed = 0;
    let mut errors = Vec::new();
    let tx = db.transaction()?;
    if user_version < 3 {
        // These indices were added in v3 (apparently)
        tx.execute_all(&[
            CREATE_OVERRIDE_HOSTNAME_INDEX_SQL,
            CREATE_DELETED_HOSTNAME_INDEX_SQL,
        ])?;
    }
    if user_version < 4 {
        // This is the update from the firefox-ios schema to our schema.
        // The `loginsSyncMeta` table was added in v4, and we moved
        // from using microseconds to milliseconds for `timeCreated`,
        // `timeLastUsed`, and `timePasswordChanged`.
        tx.execute_all(&[
            CREATE_META_TABLE_SQL,
            UPDATE_LOCAL_TIMESTAMPS_TO_MILLIS_SQL,
            UPDATE_MIRROR_TIMESTAMPS_TO_MILLIS_SQL,
        ])?;
    }
    if user_version < 5 {
        // encrypt the username/password data
        let encryptor = EncryptorDecryptor::new(encryption_key)?;
        let mut encrypt_username_and_password = |table_name: &str| -> Result<()> {
            // Encrypt the username and password field for all rows.
            let mut select_stmt = tx.prepare(&format!(
                "SELECT guid, username, password FROM {}",
                table_name
            ))?;
            let mut update_stmt = tx.prepare(&format!(
                "UPDATE {} SET username=?, password=? WHERE guid=?",
                table_name
            ))?;
            let mut delete_stmt =
                tx.prepare(&format!("DELETE FROM {} WHERE guid=?", table_name))?;

            let mut update_single_row = |guid: &str, row: &rusqlite::Row<'_>| -> Result<()> {
                let username: String = row.get(1)?;
                let password: String = row.get(2)?;
                update_stmt.execute(rusqlite::params![
                    encryptor.encrypt(&username)?,
                    encryptor.encrypt(&password)?,
                    &guid,
                ])?;
                Ok(())
            };

            // Use raw rows to avoid extra copying since we're looping over an entire table
            let mut rows = select_stmt.query(NO_PARAMS)?;
            while let Some(row) = rows.next()? {
                let guid: String = row.get(0)?;
                num_processed += 1;
                match update_single_row(&guid, &row) {
                    Ok(_) => {
                        num_succeeded += 1;
                    }
                    Err(e) => {
                        delete_stmt.execute(&[&guid])?;
                        num_failed += 1;
                        errors.push(e.to_string());
                    }
                }
            }
            Ok(())
        };

        encrypt_username_and_password("loginsL")?;
        encrypt_username_and_password("loginsM")?;

        // rename the fields
        tx.execute_all(&[
            RENAME_LOCAL_USERNAME,
            RENAME_LOCAL_PASSWORD,
            RENAME_MIRROR_USERNAME,
            RENAME_MIRROR_PASSWORD,
            RENAME_LOCAL_HOSTNAME,
            RENAME_LOCAL_SUBMIT_URL,
            RENAME_MIRROR_HOSTNAME,
            RENAME_MIRROR_SUBMIT_URL,
        ])?;
    }
    tx.execute(
        &format!(
            "PRAGMA user_version = {version}",
            version = SQLCIPHER_SWITCHOVER_VERSION
        ),
        NO_PARAMS,
    )?;
    tx.commit()?;

    Ok(MigrationMetrics {
        num_processed,
        num_succeeded,
        num_failed,
        total_duration: start_time.elapsed().as_millis() as u64,
        errors,
        ..MigrationMetrics::default()
    })
}
