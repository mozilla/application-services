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

use crate::error::*;
use lazy_static::lazy_static;
use rusqlite::Connection;
use sql_support::ConnExt;

/// Version 1: SQLCipher -> plaintext migration.
/// Version 2: addition of `loginsM.enc_unknown_fields`.
pub(super) const VERSION: i64 = 2;

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
    secFields,
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
    secFields           TEXT,
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
            is_overridden   TINYINT NOT NULL DEFAULT 0,
            -- fields on incoming records we don't know about and roundtrip.
            -- a serde_json::Value::Object as an encrypted string.
            enc_unknown_fields   TEXT
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

const CREATE_OVERRIDE_ORIGIN_INDEX_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_loginsM_is_overridden_origin
    ON loginsM (is_overridden, origin)
";

const CREATE_DELETED_ORIGIN_INDEX_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_loginsL_is_deleted_origin
    ON loginsL (is_deleted, origin)
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
fn upgrade(db: &Connection, from: i64) -> Result<()> {
    log::debug!("Upgrading schema from {} to {}", from, VERSION);
    if from == VERSION {
        return Ok(());
    }
    assert_ne!(
        from, 0,
        "Upgrading from user_version = 0 should already be handled (in `init`)"
    );

    // Schema upgrades.
    if from == 1 {
        // Just one new nullable column makes this fairly easy
        db.execute_batch("ALTER TABLE loginsM ADD enc_unknown_fields TEXT;")?;
    }
    // XXX - next migration, be sure to:
    // from = 2;
    // if from == 2 ...
    db.execute_batch(&SET_VERSION_SQL)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LoginDb;
    use rusqlite::Connection;

    #[test]
    fn test_create_schema() {
        let db = LoginDb::open_in_memory().unwrap();
        // should be VERSION.
        let version = db.query_one::<i64>("PRAGMA user_version").unwrap();
        assert_eq!(version, VERSION);
    }

    #[test]
    fn test_upgrade_v1() {
        // manually setup a V1 schema.
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS loginsM (
                    -- this was common_sql as at v1
                    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                    origin              TEXT NOT NULL,
                    httpRealm           TEXT,
                    formActionOrigin    TEXT,
                    usernameField       TEXT,
                    passwordField       TEXT,
                    timesUsed           INTEGER NOT NULL DEFAULT 0,
                    timeCreated         INTEGER NOT NULL,
                    timeLastUsed        INTEGER,
                    timePasswordChanged INTEGER NOT NULL,
                    secFields           TEXT,
                    guid                TEXT NOT NULL UNIQUE,
                    server_modified     INTEGER NOT NULL,
                    is_overridden       TINYINT NOT NULL DEFAULT 0
                    -- note enc_unknown_fields missing
                );
            ",
            )
            .unwrap();
        // Call `create` to create the rest of the schema - the "if not exists" means loginsM
        // will remain as v1.
        create(&connection).unwrap();
        // but that set the version to VERSION - set it back to 1 so our upgrade code runs.
        connection
            .execute_batch("PRAGMA user_version = 1;")
            .unwrap();

        // Now open the DB - it will create loginsL for us and migrate loginsM.
        let db = LoginDb::with_connection(connection).unwrap();
        // all migrations should have succeeded.
        let version = db.query_one::<i64>("PRAGMA user_version").unwrap();
        assert_eq!(version, VERSION);

        // and ensure sql selecting the new column works.
        db.execute_batch("SELECT enc_unknown_fields FROM loginsM")
            .unwrap();
    }
}
