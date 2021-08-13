/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Code to migrate from an sqlcipher DB to a plaintext DB

use crate::db::{MigrationMetrics, MigrationPhaseMetrics};
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::sync::SyncStatus;
use crate::sync::{LocalLogin, MirrorLogin};
use crate::util;
use crate::Login;
use crate::LoginStore;
use rusqlite::{named_params, Connection, Row, NO_PARAMS};
use sql_support::ConnExt;
use std::path::Path;
use std::time::{Duration, Instant};
use sync15::ServerTimestamp;
use sync_guid::Guid;

pub fn migrate_sqlcipher_db_to_plaintext(
    old_db_path: impl AsRef<Path>,
    new_db_path: impl AsRef<Path>,
    old_encryption_key: &str,
    new_encryption_key: &str,
    salt: Option<&str>,
) -> Result<MigrationMetrics> {
    let mut db = Connection::open(old_db_path)?;
    init_sqlcipher_db(&mut db, old_encryption_key, salt)?;

    // Init the new plaintext db as we would a regular client
    let new_db_store = LoginStore::new(new_db_path)?;
    let metrics = migrate_from_sqlcipher_db(&mut db, new_db_store, new_encryption_key)?;

    Ok(metrics)
}

fn init_sqlcipher_db(db: &mut Connection, encryption_key: &str, salt: Option<&str>) -> Result<()> {
    // Most of this code was copied from the old LoginDB::with_connection() method.
    db.set_pragma("key", encryption_key)?
        .set_pragma("secure_delete", true)?;
    sqlcipher_3_compat(db)?;

    if let Some(s) = salt {
        // IOS clients need to manually specify the salt to work around locking issues.  If the
        // salt was passed in, assume that we also want to set cipher_plaintext_header_size.  See
        // https://www.zetetic.net/sqlcipher/sqlcipher-api/#cipher_plaintext_header_size.
        db.set_pragma("cipher_plaintext_header_size", 32)?;
        db.set_pragma("cipher_salt", format!("x'{}'", s))?;
    }

    // `temp_store = 2` is required on Android to force the DB to keep temp
    // files in memory, since on Android there's no tmp partition. See
    // https://github.com/mozilla/mentat/issues/505. Ideally we'd only
    // do this on Android, or allow caller to configure it.
    db.set_pragma("temp_store", 2)?;
    Ok(())
}

fn sqlcipher_3_compat(conn: &Connection) -> Result<()> {
    // SQLcipher pre-4.0.0 compatibility. Using SHA1 still
    // is less than ideal, but should be fine. Real uses of
    // this (lockwise, etc) use a real random string for the
    // encryption key, so the reduced KDF iteration count
    // is fine.
    conn.set_pragma("cipher_page_size", 1024)?
        .set_pragma("kdf_iter", 64000)?
        .set_pragma("cipher_hmac_algorithm", "HMAC_SHA1")?
        .set_pragma("cipher_kdf_algorithm", "PBKDF2_HMAC_SHA1")?;
    Ok(())
}

//Manually copy over row by row from sqlcipher db to a plaintext db
pub fn migrate_from_sqlcipher_db(
    cipher_conn: &mut Connection,
    new_db_store: LoginStore,
    encryption_key: &str,
) -> Result<MigrationMetrics> {
    // encrypt the username/password data
    let encryptor = EncryptorDecryptor::new(encryption_key)?;

    // Migrate tables separately due to specific columns in each needing
    // to be ported over
    let local_metrics = migrate_local_logins(&cipher_conn, &new_db_store, &encryptor)?;
    let mirror_metrics = migrate_mirror_logins(&cipher_conn, &new_db_store, &encryptor)?;
    migrate_sync_metadata(&cipher_conn, &new_db_store)?;

    // A little ugly but necessary due to individual migrations of both tables
    Ok(MigrationMetrics {
        fixup_phase: MigrationPhaseMetrics {
            num_processed: local_metrics.fixup_phase.num_processed
                + mirror_metrics.fixup_phase.num_processed,
            num_succeeded: local_metrics.fixup_phase.num_succeeded
                + mirror_metrics.fixup_phase.num_succeeded,
            num_failed: local_metrics.fixup_phase.num_failed
                + mirror_metrics.fixup_phase.num_failed,
            total_duration: local_metrics.fixup_phase.total_duration
                + mirror_metrics.fixup_phase.total_duration,
            errors: [
                &local_metrics.fixup_phase.errors[..],
                &mirror_metrics.fixup_phase.errors[..],
            ]
            .concat(),
        },
        insert_phase: MigrationPhaseMetrics {
            num_processed: local_metrics.insert_phase.num_processed
                + mirror_metrics.insert_phase.num_processed,
            num_succeeded: local_metrics.insert_phase.num_succeeded
                + mirror_metrics.insert_phase.num_succeeded,
            num_failed: local_metrics.insert_phase.num_failed
                + mirror_metrics.insert_phase.num_failed,
            total_duration: local_metrics.insert_phase.total_duration
                + mirror_metrics.insert_phase.total_duration,
            errors: [
                &local_metrics.insert_phase.errors[..],
                &mirror_metrics.insert_phase.errors[..],
            ]
            .concat(),
        },
        num_processed: local_metrics.num_processed + mirror_metrics.num_processed,
        num_succeeded: local_metrics.num_succeeded + mirror_metrics.num_succeeded,
        num_failed: local_metrics.num_failed + mirror_metrics.num_failed,
        total_duration: local_metrics.total_duration + mirror_metrics.total_duration,
        errors: [&local_metrics.errors[..], &mirror_metrics.errors[..]].concat(),
    })
}

fn migrate_sync_metadata(conn: &Connection, store: &LoginStore) -> Result<()> {
    let mut select_stmt = conn.prepare("SELECT * FROM loginsSyncMeta")?;
    let mut rows = select_stmt.query(NO_PARAMS)?;

    while let Some(row) = rows.next()? {
        let key: String = row.get("key")?;
        let value: String = row.get("value")?;

        store.db.lock().unwrap().execute_named(
            "INSERT INTO loginsSyncMeta (key, value) VALUES (:key, :value)",
            named_params! { ":key": &key, ":value": &value },
        )?;
    }
    Ok(())
}

// This was copied from import_multiple in db.rs with a focus on LocalLogin
pub fn migrate_local_logins(
    cipher_conn: &Connection,
    store: &LoginStore,
    encryptor: &EncryptorDecryptor,
) -> Result<MigrationMetrics> {
    let logins = get_local_logins(&cipher_conn, &encryptor)?;

    let new_db = store.db.lock().unwrap();
    let conn = new_db.conn();
    let tx = conn.unchecked_transaction()?;

    let import_start = Instant::now();
    let sql = "INSERT OR IGNORE INTO loginsL (
            origin,
            httpRealm,
            formActionOrigin,
            usernameField,
            passwordField,
            timesUsed,
            usernameEnc,
            passwordEnc,
            guid,
            timeCreated,
            timeLastUsed,
            timePasswordChanged,
            local_modified,
            is_deleted,
            sync_status
        ) VALUES (
            :origin,
            :http_realm,
            :form_action_origin,
            :username_field,
            :password_field,
            :times_used,
            :username_enc,
            :password_enc,
            :guid,
            :time_created,
            :time_last_used,
            :time_password_changed,
            :local_modified,
            :is_deleted,
            :sync_status
        )";
    let import_start_total_logins: u64 = logins.len() as u64;
    let mut num_failed_fixup: u64 = 0;
    let mut num_failed_insert: u64 = 0;
    let mut fixup_phase_duration = Duration::new(0, 0);
    let mut fixup_errors: Vec<String> = Vec::new();
    let mut insert_errors: Vec<String> = Vec::new();

    for local_login in logins {
        // This is a little bit of hoop-jumping to avoid cloning each borrowed item
        // in order to *possibly* created a fixed-up version.
        let mut login = local_login.login;
        let maybe_fixed_login = login.maybe_fixup().and_then(|fixed| {
            match &fixed {
                None => new_db.check_for_dupes(&login)?,
                Some(l) => new_db.check_for_dupes(&l)?,
            };
            Ok(fixed)
        });
        match maybe_fixed_login {
            Ok(None) => {} // The provided login was fine all along
            Ok(Some(l)) => {
                // We made a new, fixed-up Login.
                login = l;
            }
            Err(e) => {
                log::warn!("Skipping login {} as it is invalid ({}).", login.guid(), e);
                fixup_errors.push(e.label().into());
                num_failed_fixup += 1;
                continue;
            }
        };
        // Now we can safely insert it, knowing that it's valid data.
        let old_guid = login.guid(); // Keep the old GUID around so we can debug errors easily.
        let guid = if old_guid.is_valid_for_sync_server() {
            old_guid.clone()
        } else {
            Guid::random()
        };
        fixup_phase_duration = import_start.elapsed();
        match conn.execute_named_cached(
            &sql,
            named_params! {
                ":origin": login.origin,
                ":http_realm": login.http_realm,
                ":form_action_origin": login.form_action_origin,
                ":username_field": login.username_field,
                ":password_field": login.password_field,
                ":username_enc": login.username_enc,
                ":password_enc": login.password_enc,
                ":guid": guid,
                ":time_created": login.time_created,
                ":times_used": login.times_used,
                ":time_last_used": login.time_last_used,
                ":time_password_changed": login.time_password_changed,
                // Local login specific stuff
                ":local_modified": util::system_time_ms_i64(local_login.local_modified),
                ":is_deleted": local_login.is_deleted,
                ":sync_status": local_login.sync_status as u8
            },
        ) {
            Ok(_) => log::info!("Imported {} (new GUID {}) successfully.", old_guid, guid),
            Err(e) => {
                log::warn!("Could not import {} ({}).", old_guid, e);
                insert_errors.push(Error::from(e).label().into());
                num_failed_insert += 1;
            }
        };
    }
    tx.commit()?;

    let num_post_fixup = import_start_total_logins - num_failed_fixup;
    let num_failed = num_failed_fixup + num_failed_insert;
    let insert_phase_duration = import_start
        .elapsed()
        .checked_sub(fixup_phase_duration)
        .unwrap_or_else(|| Duration::new(0, 0));
    let mut all_errors = Vec::new();
    all_errors.extend(fixup_errors.clone());
    all_errors.extend(insert_errors.clone());
    let metrics = MigrationMetrics {
        fixup_phase: MigrationPhaseMetrics {
            num_processed: import_start_total_logins,
            num_succeeded: num_post_fixup,
            num_failed: num_failed_fixup,
            total_duration: fixup_phase_duration.as_millis() as u64,
            errors: fixup_errors,
        },
        insert_phase: MigrationPhaseMetrics {
            num_processed: num_post_fixup,
            num_succeeded: num_post_fixup - num_failed_insert,
            num_failed: num_failed_insert,
            total_duration: insert_phase_duration.as_millis() as u64,
            errors: insert_errors,
        },
        num_processed: import_start_total_logins,
        num_succeeded: import_start_total_logins - num_failed,
        num_failed,
        total_duration: fixup_phase_duration
            .checked_add(insert_phase_duration)
            .unwrap_or_else(|| Duration::new(0, 0))
            .as_millis() as u64,
        errors: all_errors,
    };
    log::info!(
        "Finished importing logins with the following metrics: {:#?}",
        metrics
    );
    Ok(metrics)
}

fn get_local_logins(conn: &Connection, encryptor: &EncryptorDecryptor) -> Result<Vec<LocalLogin>> {
    let mut select_stmt = conn.prepare("SELECT * FROM loginsL")?;
    let mut rows = select_stmt.query(NO_PARAMS)?;
    let mut local_logins: Vec<LocalLogin> = Vec::new();
    // Use raw rows to avoid extra copying since we're looping over an entire table
    while let Some(row) = rows.next()? {
        match get_login_from_row(row, &encryptor) {
            Ok(login) => {
                // This is very close to what is in merge.rs from_row but this login is the old schema
                let l_login = LocalLogin {
                    login,
                    local_modified: util::system_time_millis_from_row(row, "local_modified")?,
                    is_deleted: row.get("is_deleted")?,
                    sync_status: SyncStatus::from_u8(row.get("sync_status")?)?,
                };
                local_logins.push(l_login);
            }
            Err(e) => {
                // We should probably just skip if we can't successfully fetch the row
                println!("{:?}", e);
            }
        }
    }
    Ok(local_logins)
}

fn get_mirror_logins(
    conn: &Connection,
    encryptor: &EncryptorDecryptor,
) -> Result<Vec<MirrorLogin>> {
    let mut select_stmt = conn.prepare("SELECT * FROM loginsM")?;
    let mut rows = select_stmt.query(NO_PARAMS)?;
    let mut mirror_logins: Vec<MirrorLogin> = Vec::new();
    // Use raw rows to avoid extra copying since we're looping over an entire table
    while let Some(row) = rows.next()? {
        match get_login_from_row(row, &encryptor) {
            Ok(login) => {
                // This is very close to what is in merge.rs from_row but this login is the old schema
                let m_login = MirrorLogin {
                    login,
                    server_modified: ServerTimestamp(row.get::<_, i64>("server_modified")?),
                    is_overridden: row.get("is_overridden")?,
                };
                mirror_logins.push(m_login);
            }
            Err(e) => {
                // We should probably just skip if we can't successfully fetch the row
                println!("{:?}", e);
            }
        }
    }
    Ok(mirror_logins)
}

// This was lifted from import_multiple in db.rs with a focus on LocalLogin
pub fn migrate_mirror_logins(
    cipher_conn: &Connection,
    store: &LoginStore,
    encryptor: &EncryptorDecryptor,
) -> Result<MigrationMetrics> {
    let logins = get_mirror_logins(&cipher_conn, &encryptor)?;

    let new_db = store.db.lock().unwrap();
    let conn = new_db.conn();
    let tx = conn.unchecked_transaction()?;

    let import_start = Instant::now();
    let sql = "INSERT OR IGNORE INTO loginsM (
            origin,
            httpRealm,
            formActionOrigin,
            usernameField,
            passwordField,
            timesUsed,
            usernameEnc,
            passwordEnc,
            guid,
            timeCreated,
            timeLastUsed,
            timePasswordChanged,
            server_modified,
            is_overridden
        ) VALUES (
            :origin,
            :http_realm,
            :form_action_origin,
            :username_field,
            :password_field,
            :times_used,
            :username_enc,
            :password_enc,
            :guid,
            :time_created,
            :time_last_used,
            :time_password_changed,
            :server_modified,
            :is_overridden
        )";
    let import_start_total_logins: u64 = logins.len() as u64;
    let mut num_failed_fixup: u64 = 0;
    let mut num_failed_insert: u64 = 0;
    let mut fixup_phase_duration = Duration::new(0, 0);
    let mut fixup_errors: Vec<String> = Vec::new();
    let mut insert_errors: Vec<String> = Vec::new();

    for mirror_login in logins {
        // This is a little bit of hoop-jumping to avoid cloning each borrowed item
        // in order to *possibly* created a fixed-up version.
        let mut login = mirror_login.login;
        let maybe_fixed_login = login.maybe_fixup().and_then(|fixed| {
            match &fixed {
                None => new_db.check_for_dupes(&login)?,
                Some(l) => new_db.check_for_dupes(&l)?,
            };
            Ok(fixed)
        });
        match maybe_fixed_login {
            Ok(None) => {} // The provided login was fine all along
            Ok(Some(l)) => {
                // We made a new, fixed-up Login.
                login = l;
            }
            Err(e) => {
                log::warn!("Skipping login {} as it is invalid ({}).", login.guid(), e);
                fixup_errors.push(e.label().into());
                num_failed_fixup += 1;
                continue;
            }
        };
        // Now we can safely insert it, knowing that it's valid data.
        let old_guid = login.guid(); // Keep the old GUID around so we can debug errors easily.
        let guid = if old_guid.is_valid_for_sync_server() {
            old_guid.clone()
        } else {
            Guid::random()
        };
        fixup_phase_duration = import_start.elapsed();
        match conn.execute_named_cached(
            &sql,
            named_params! {
                ":origin": login.origin,
                ":http_realm": login.http_realm,
                ":form_action_origin": login.form_action_origin,
                ":username_field": login.username_field,
                ":password_field": login.password_field,
                ":username_enc": login.username_enc,
                ":password_enc": login.password_enc,
                ":guid": guid,
                ":time_created": login.time_created,
                ":times_used": login.times_used,
                ":time_last_used": login.time_last_used,
                ":time_password_changed": login.time_password_changed,
                // Mirror login specific stuff
                ":server_modified": mirror_login.server_modified.as_millis(),
                ":is_overridden": mirror_login.is_overridden,
            },
        ) {
            Ok(_) => log::info!("Imported {} (new GUID {}) successfully.", old_guid, guid),
            Err(e) => {
                log::warn!("Could not import {} ({}).", old_guid, e);
                insert_errors.push(Error::from(e).label().into());
                num_failed_insert += 1;
            }
        };
    }
    tx.commit()?;

    let num_post_fixup = import_start_total_logins - num_failed_fixup;
    let num_failed = num_failed_fixup + num_failed_insert;
    let insert_phase_duration = import_start
        .elapsed()
        .checked_sub(fixup_phase_duration)
        .unwrap_or_else(|| Duration::new(0, 0));
    let mut all_errors = Vec::new();
    all_errors.extend(fixup_errors.clone());
    all_errors.extend(insert_errors.clone());

    let metrics = MigrationMetrics {
        fixup_phase: MigrationPhaseMetrics {
            num_processed: import_start_total_logins,
            num_succeeded: num_post_fixup,
            num_failed: num_failed_fixup,
            total_duration: fixup_phase_duration.as_millis() as u64,
            errors: fixup_errors,
        },
        insert_phase: MigrationPhaseMetrics {
            num_processed: num_post_fixup,
            num_succeeded: num_post_fixup - num_failed_insert,
            num_failed: num_failed_insert,
            total_duration: insert_phase_duration.as_millis() as u64,
            errors: insert_errors,
        },
        num_processed: import_start_total_logins,
        num_succeeded: import_start_total_logins - num_failed,
        num_failed,
        total_duration: fixup_phase_duration
            .checked_add(insert_phase_duration)
            .unwrap_or_else(|| Duration::new(0, 0))
            .as_millis() as u64,
        errors: all_errors,
    };
    log::info!(
        "Finished importing logins with the following metrics: {:#?}",
        metrics
    );
    Ok(metrics)
}

// Convert rows from old schema to match new fields in the Login struct
fn get_login_from_row(row: &Row<'_>, encryptor: &EncryptorDecryptor) -> Result<Login> {
    // We want to grab the "old" schema
    let guid: String = row.get("guid")?;
    let username: String = row.get("username").unwrap_or_default();
    let password: String = row.get("password")?;
    // migrating hostname to the new column origin
    let origin: String = row.get("hostname")?;
    let http_realm: Option<String> = row.get("httpRealm")?;
    // migrating formSubmitURL to the new column action origin
    let form_action_origin: Option<String> = row.get("formSubmitURL")?;
    let username_field: Option<String> = row.get("usernameField")?;
    let password_field: Option<String> = row.get("passwordField")?;
    let time_created: i64 = row.get("timeCreated")?;
    let time_last_used: i64 = row.get("timeLastUsed").unwrap_or_default();
    let time_password_changed: i64 = row.get("timePasswordChanged")?;
    let times_used: i64 = row.get("timesUsed")?;

    let login: Login = Login {
        id: guid,
        username_enc: encryptor.encrypt(&username)?,
        password_enc: encryptor.encrypt(&password)?,
        origin,
        http_realm,
        form_action_origin,
        username_field: username_field.unwrap_or_default(),
        password_field: password_field.unwrap_or_default(),
        time_created,
        time_last_used,
        time_password_changed,
        times_used,
    };
    Ok(login)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LoginDb;
    use crate::encryption::test_utils::{decrypt, TEST_ENCRYPTION_KEY};
    use crate::schema;
    use rusqlite::types::ValueRef;
    use std::path::PathBuf;

    static TEST_SALT: &str = "01010101010101010101010101010101";

    fn open_old_db(db_path: impl AsRef<Path>, salt: Option<&str>) -> Connection {
        let mut db = Connection::open(db_path).unwrap();
        init_sqlcipher_db(&mut db, "old-key", salt).unwrap();
        sqlcipher_3_compat(&db).unwrap();
        db
    }

    fn create_old_db(db_path: impl AsRef<Path>, salt: Option<&str>) {
        let mut db = open_old_db(db_path, salt);
        let tx = db.transaction().unwrap();
        schema::init(&tx).unwrap();

        // Manually migrate back to schema v4 and insert some data
        // These all need to be executed as separate statements or
        // sqlite will not execute them
        const RENAME_LOCAL_USERNAME: &str = "
            ALTER TABLE loginsL RENAME usernameEnc to username;
        ";

        const RENAME_LOCAL_PASSWORD: &str = "
            ALTER TABLE loginsL RENAME passwordEnc to password;
        ";

        const RENAME_MIRROR_USERNAME: &str = "
            ALTER TABLE loginsM RENAME usernameEnc to username
        ";

        const RENAME_MIRROR_PASSWORD: &str = "
            ALTER TABLE loginsM RENAME passwordEnc to password
        ";

        const RENAME_LOCAL_HOSTNAME: &str = "
            ALTER TABLE loginsL RENAME origin TO hostname
        ";

        const RENAME_LOCAL_SUBMIT_URL: &str = "
            ALTER TABLE loginsL RENAME formActionOrigin TO formSubmitURL
        ";

        const RENAME_MIRROR_HOSTNAME: &str = "
            ALTER TABLE loginsM RENAME origin TO hostname
        ";

        const RENAME_MIRROR_SUBMIT_URL: &str = "
            ALTER TABLE loginsM RENAME formActionOrigin TO formSubmitURL
        ";

        const INSERT_LOGINS_L: &str = "
            INSERT INTO loginsL(guid, username, password, hostname,
                httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
                timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
                VALUES
                ('a', 'test', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 2),
                ('b', 'test', 'password', 'https://www.example.com', 'https://www.example.com', 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 2);
        ";

        const INSERT_LOGINS_M: &str = "
            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
                usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
                is_overridden, server_modified)
                VALUES
                ('b', 'test', 'password', 'https://www.example.com', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 0, 1000),
                ('c', 'test', 'password', 'http://example.com:1234/', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 0, 1000),
                ('d', 'test', 'password', '', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 1, 1000);
        ";

        // Need to test migrating sync meta else we'll be resyncing everything
        tx.execute_named(
            "INSERT INTO loginsSyncMeta (key, value)
             VALUES (:key, :value)",
            rusqlite::named_params! {
                ":key": "last_sync",
                ":value": "some_payload_data",
            },
        )
        .unwrap();
        tx.execute_all(&[
            RENAME_LOCAL_USERNAME,
            RENAME_MIRROR_USERNAME,
            RENAME_LOCAL_PASSWORD,
            RENAME_MIRROR_PASSWORD,
            RENAME_LOCAL_HOSTNAME,
            RENAME_MIRROR_HOSTNAME,
            RENAME_LOCAL_SUBMIT_URL,
            RENAME_MIRROR_SUBMIT_URL,
            // Inserts
            INSERT_LOGINS_L,
            INSERT_LOGINS_M,
        ])
        .unwrap();
        tx.commit().unwrap();
    }

    struct TestPaths {
        _tempdir: tempfile::TempDir,
        old_db: PathBuf,
        new_db: PathBuf,
    }

    impl TestPaths {
        fn new() -> Self {
            let tempdir = tempfile::tempdir().unwrap();
            Self {
                old_db: tempdir.path().join(Path::new("old-db.db")),
                new_db: tempdir.path().join(Path::new("new-db.db")),
                _tempdir: tempdir,
            }
        }
    }

    fn check_migrated_data(db: &LoginDb) {
        let mut stmt = db
            .prepare("SELECT * FROM loginsL where guid = 'a'")
            .unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();
        assert_eq!(
            decrypt(row.get_raw("usernameEnc").as_str().unwrap()),
            "test"
        );
        assert_eq!(
            decrypt(row.get_raw("passwordEnc").as_str().unwrap()),
            "password"
        );
        assert_eq!(
            row.get_raw("origin").as_str().unwrap(),
            "https://www.example.com"
        );
        assert_eq!(row.get_raw("httpRealm"), ValueRef::Null);
        assert_eq!(
            row.get_raw("formActionOrigin").as_str().unwrap(),
            "https://www.example.com"
        );
        assert_eq!(row.get_raw("usernameField").as_str().unwrap(), "username");
        assert_eq!(row.get_raw("passwordField").as_str().unwrap(), "password");
        assert_eq!(row.get_raw("timeCreated").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timeLastUsed").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timePasswordChanged").as_i64().unwrap(), 1);
        assert_eq!(row.get_raw("timesUsed").as_i64().unwrap(), 10);
        assert_eq!(row.get_raw("is_deleted").as_i64().unwrap(), 0);
        assert_eq!(row.get_raw("sync_status").as_i64().unwrap(), 2);

        let mut stmt = db
            .prepare("SELECT * FROM loginsM WHERE guid = 'b'")
            .unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();
        assert_eq!(
            decrypt(row.get_raw("usernameEnc").as_str().unwrap()),
            "test"
        );
        assert_eq!(
            decrypt(row.get_raw("passwordEnc").as_str().unwrap()),
            "password"
        );
        assert_eq!(
            row.get_raw("origin").as_str().unwrap(),
            "https://www.example.com"
        );
        assert_eq!(row.get_raw("httpRealm").as_str().unwrap(), "Test Realm");
        assert_eq!(row.get_raw("formActionOrigin"), ValueRef::Null);
        assert_eq!(row.get_raw("usernameField").as_str().unwrap(), "");
        assert_eq!(row.get_raw("passwordField").as_str().unwrap(), "");
        assert_eq!(row.get_raw("timeCreated").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timeLastUsed").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timePasswordChanged").as_i64().unwrap(), 1);
        assert_eq!(row.get_raw("timesUsed").as_i64().unwrap(), 10);

        assert_eq!(row.get_raw("is_overridden").as_i64().unwrap(), 0);
        assert_eq!(row.get_raw("server_modified").as_i64().unwrap(), 1000);

        // Ensure loginsSyncMeta migrated correctly
        let mut stmt = db.prepare("SELECT * FROM loginsSyncMeta").unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();

        assert_eq!(row.get_raw("key").as_str().unwrap(), "last_sync");
        assert_eq!(row.get_raw("value").as_str().unwrap(), "some_payload_data");

        // The schema version should reset to 1 after the migration
        assert_eq!(db.query_one::<i64>("PRAGMA user_version").unwrap(), 1);
    }

    #[ignore]
    #[test]
    fn test_migrate_data() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), None);
        let metrics = migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            None,
        )
        .unwrap();

        // Check that the data from the old db is present in the the new DB
        let db = LoginDb::open(testpaths.new_db).unwrap();
        check_migrated_data(&db);

        // Check migration numbers
        assert_eq!(metrics.num_processed, 5);
        assert_eq!(metrics.num_succeeded, 4);
        assert_eq!(metrics.num_failed, 1);
        assert_eq!(metrics.errors, ["InvalidLogin::EmptyOrigin"]);
    }

    #[ignore]
    #[test]
    fn test_migration_errors() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), None);
        let old_db = open_old_db(testpaths.old_db.as_path(), None);
        old_db
            .execute(
                "UPDATE loginsM SET username = NULL WHERE guid='b'",
                NO_PARAMS,
            )
            .unwrap();
        drop(old_db);

        let metrics = migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            None,
        )
        .unwrap();

        // Check that only the non-errors are in the new DB
        let db = LoginDb::open(testpaths.new_db).unwrap();
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsL").unwrap(),
            2
        );
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsM").unwrap(),
            2
        );

        // Check metrics
        assert_eq!(metrics.num_processed, 5);
        assert_eq!(metrics.num_succeeded, 4);
        assert_eq!(metrics.num_failed, 1);
        assert_eq!(metrics.errors.len(), 1);
    }

    #[ignore]
    #[test]
    fn test_migrate_with_manual_salt() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), Some(TEST_SALT));
        migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            Some(TEST_SALT),
        )
        .unwrap();
        let db = LoginDb::open(testpaths.new_db).unwrap();
        check_migrated_data(&db);
    }
}
