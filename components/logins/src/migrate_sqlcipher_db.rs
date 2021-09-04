/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Code to migrate from an sqlcipher DB to a plaintext DB

use crate::db::{MigrationMetrics, MigrationPhaseMetrics};
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::sync::{LocalLogin, MirrorLogin, SyncStatus};
use crate::util;
use crate::LoginStore;
use crate::{EncryptedLogin, Login, LoginFields, RecordFields, SecureLoginFields};
use rusqlite::{named_params, Connection, Row, NO_PARAMS};
use sql_support::ConnExt;
use std::collections::HashMap;
use std::ops::Add;
use std::path::Path;
use std::time::{Instant, SystemTime};
use sync15::ServerTimestamp;

#[derive(Debug)]
struct MigrationPlan {
    // We use guid as the identifier since MigrationLogin has both local and mirror
    logins: HashMap<String, MigrationLogin>,
}

impl MigrationPlan {
    fn new() -> MigrationPlan {
        MigrationPlan {
            logins: HashMap::new(),
        }
    }

    fn fix_mismatched_records(mut self) -> Result<Self> {
        let mut guids_to_remove: Vec<String> = Vec::new();
        for (guid, login) in &self.logins {
            // Case 1: If the mirror record has is_overridden and no local records -> delete
            if let Some(mirror_login) = &login.mirror_login {
                if mirror_login.is_overridden && login.local_login.is_none() {
                    guids_to_remove.push(guid.to_string());
                }
            }
        }
        // Secondary loop to prevent mutating while iterating the hashmap
        for guid in guids_to_remove {
            // Delete the record
            log::warn!("Mirror was overridden but no local record was found. Deleting...");
            self.logins.remove(&guid);
        }
        Ok(self)
    }
}

#[derive(Debug)]
struct MigrationLogin {
    local_login: Option<LocalLogin>,
    mirror_login: Option<MirrorLogin>,
    status: MigrationStatus,
}

// TODO: Kept this here as part of the initial design but doesn't seem needed as we go through this
#[derive(Debug)]
enum MigrationStatus {
    Processing,
    // Success,
    // Fixed,
    // Failed,
}

// Simplify the code for combining migration metrics
// the impl is only in this file as this should dissapear once we're done with sql migrations
impl Add for MigrationMetrics {
    type Output = MigrationMetrics;
    fn add(self, rhs: MigrationMetrics) -> MigrationMetrics {
        MigrationMetrics {
            insert_phase: self.insert_phase + rhs.insert_phase,
            fixup_phase: self.fixup_phase + rhs.fixup_phase,
            num_processed: self.num_processed + rhs.num_processed,
            num_succeeded: self.num_succeeded + rhs.num_succeeded,
            num_failed: self.num_failed + rhs.num_failed,
            total_duration: self.total_duration + rhs.total_duration,
            errors: [&self.errors[..], &rhs.errors[..]].concat(),
        }
    }
}

impl Add for MigrationPhaseMetrics {
    type Output = MigrationPhaseMetrics;
    fn add(self, rhs: MigrationPhaseMetrics) -> MigrationPhaseMetrics {
        MigrationPhaseMetrics {
            num_processed: self.num_processed + rhs.num_processed,
            num_succeeded: self.num_succeeded + rhs.num_succeeded,
            num_failed: self.num_failed + rhs.num_failed,
            total_duration: self.total_duration + rhs.total_duration,
            errors: [&self.errors[..], &rhs.errors[..]].concat(),
        }
    }
}

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
    let encdec = EncryptorDecryptor::new(encryption_key)?;

    // Step 1
    let mut migration_plan: MigrationPlan = generate_plan_from_db(&cipher_conn, &encdec)?;

    // Step 2: Handle mismatch records, edge cases before fixup
    migration_plan = migration_plan.fix_mismatched_records()?;

    // Step 3: Apply fixups to LOCAL logins

    // TODO Step 4: Insert (raw SQL) into new DB
    migrate_logins(&migration_plan, &new_db_store, &encdec)?;

    let metadata_metrics = migrate_sync_metadata(&cipher_conn, &new_db_store)?;

    // TODO: Replace this with all metrics
    Ok(metadata_metrics)
}

fn generate_plan_from_db(
    cipher_conn: &Connection,
    encryptor: &EncryptorDecryptor,
) -> Result<MigrationPlan> {
    let mut migration_plan = MigrationPlan::new();

    // Process local logins and add to MigrationPlan
    let mut local_stmt = cipher_conn.prepare("SELECT * FROM loginsL")?;
    let mut local_rows = local_stmt.query(NO_PARAMS)?;
    while let Some(row) = local_rows.next()? {
        match get_login_from_row(row) {
            Ok(login) => {
                let l_login = LocalLogin {
                    login: login.encrypt(&encryptor)?,
                    local_modified: util::system_time_millis_from_row(row, "local_modified")
                        .unwrap_or(SystemTime::now()),
                    is_deleted: row.get("is_deleted").unwrap_or_default(),
                    sync_status: SyncStatus::from_u8(row.get("sync_status").unwrap_or_default())
                        .unwrap_or(SyncStatus::New),
                };
                let key = l_login.login.record.id.clone();
                migration_plan
                    .logins
                    .entry(key)
                    .and_modify(|l| l.local_login = Some(l_login.clone()))
                    .or_insert(MigrationLogin {
                        //guid: key,
                        local_login: Some(l_login),
                        mirror_login: None,
                        status: MigrationStatus::Processing,
                    });
            }
            Err(e) => {
                // We should probably just skip if we can't successfully fetch the row
                log::warn!("Error getting record from DB: {:?}", e);
            }
        }
    }
    // Process mirror logins and add to MigrationPlan
    let mut mirror_stmt = cipher_conn.prepare("SELECT * FROM loginsM")?;
    let mut mirror_rows = mirror_stmt.query(NO_PARAMS)?;
    while let Some(row) = mirror_rows.next()? {
        match get_login_from_row(row) {
            Ok(login) => {
                let m_login = MirrorLogin {
                    login: login.encrypt(&encryptor)?,
                    server_modified: ServerTimestamp(
                        row.get::<_, i64>("server_modified").unwrap_or_default(),
                    ),
                    is_overridden: row.get("is_overridden").unwrap_or_default(),
                };

                let key = m_login.login.record.id.clone();
                migration_plan
                    .logins
                    .entry(key)
                    .and_modify(|l| l.mirror_login = Some(m_login.clone()))
                    .or_insert(MigrationLogin {
                        //guid: key,
                        local_login: None,
                        mirror_login: Some(m_login),
                        status: MigrationStatus::Processing,
                    });
            }
            Err(e) => {
                // We should probably just skip if we can't successfully fetch the row
                log::warn!("Error getting record from DB: {:?}", e);
            }
        }
    }
    Ok(migration_plan)
}

fn migrate_logins(
    migration_plan: &MigrationPlan,
    store: &LoginStore,
    encdec: &EncryptorDecryptor,
) -> Result<MigrationMetrics> {
    let import_start = Instant::now();
    let import_start_total_logins: u64 = migration_plan.logins.len() as u64;
    let mut num_failed_insert: u64 = 0;
    let mut insert_errors: Vec<String> = Vec::new();

    for (_, login) in &migration_plan.logins {
        // Migrate local login first
        if let Some(local_login) = &login.local_login {
            match migrate_local_login(store, local_login, &encdec) {
                Ok(_) => {
                    if let Some(mirror_login) = &login.mirror_login {
                        // If successful, then migrate mirror
                        migrate_mirror_login(&store, &mirror_login)?;
                    }
                }
                Err(e) => {
                    // If not successful on local, but we have a mirror
                    // If there is an override -> delete skip both
                    // IF no override -> attempt to import as normal
                    if let Some(mirror_login) = &login.mirror_login {
                        if mirror_login.is_overridden {
                            num_failed_insert += 1;
                            insert_errors.push(Error::from(e).label().into());
                            continue;
                        } else {
                            migrate_mirror_login(&store, &mirror_login)?;
                        }
                    }
                }
            }
        // If we just have mirror, import as normal
        } else {
            if let Some(mirror_login) = &login.mirror_login {
                match migrate_mirror_login(&*store, &mirror_login) {
                    Ok(_) => {}
                    Err(e) => {
                        num_failed_insert += 1;
                        insert_errors.push(Error::from(e).label().into());
                    }
                }
            }
        }
    }
    let insert_phase_duration = import_start.elapsed();
    let mut all_errors = Vec::new();
    all_errors.extend(insert_errors.clone());
    let metrics = MigrationMetrics {
        fixup_phase: MigrationPhaseMetrics {
            num_processed: 0,
            num_succeeded: 0,
            num_failed: 0,
            total_duration: 0,
            errors: Vec::new(),
        },
        insert_phase: MigrationPhaseMetrics {
            num_processed: import_start_total_logins,
            num_succeeded: import_start_total_logins - num_failed_insert,
            num_failed: num_failed_insert,
            total_duration: insert_phase_duration.as_millis() as u64,
            errors: insert_errors,
        },
        num_processed: import_start_total_logins,
        num_succeeded: import_start_total_logins - num_failed_insert,
        num_failed: num_failed_insert,
        total_duration: insert_phase_duration.as_millis() as u64,
        errors: all_errors,
    };
    log::info!(
        "Finished importing logins with the following metrics: {:#?}",
        metrics
    );
    Ok(metrics)
}

// This was copied from import_multiple in db.rs with a focus on LocalLogin
fn migrate_local_login(
    store: &LoginStore,
    local_login: &LocalLogin,
    encdec: &EncryptorDecryptor,
) -> Result<()> {
    let new_db = store.db.lock().unwrap();
    let conn = new_db.conn();
    let tx = conn.unchecked_transaction()?;

    let sql = "INSERT OR IGNORE INTO loginsL (
            origin,
            httpRealm,
            formActionOrigin,
            usernameField,
            passwordField,
            timesUsed,
            secFields,
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
            :sec_fields,
            :guid,
            :time_created,
            :time_last_used,
            :time_password_changed,
            :local_modified,
            :is_deleted,
            :sync_status
        )";

    // TODO: Identify a way to prevent cloning on every row
    let login = &local_login.login.clone().decrypt(&encdec)?;
    let login = match new_db.fixup_and_check_for_dupes(&login.guid(), login.entry(), encdec) {
        Ok(new_entry) => EncryptedLogin {
            //record doesn't get fixed up
            record: RecordFields {
                id: local_login.login.record.id.to_string(),
                ..local_login.login.record
            },
            fields: new_entry.fields,
            sec_fields: new_entry.sec_fields.encrypt(encdec)?,
        },
        Err(e) => {
            log::warn!(
                "Skipping login {} as it is invalid ({}).",
                &local_login.login.guid(),
                e
            );
            return Err(e);
        }
    };

    match conn.execute_named_cached(
        &sql,
        named_params! {
            ":origin": login.fields.origin,
            ":http_realm": login.fields.http_realm,
            ":form_action_origin": login.fields.form_action_origin,
            ":username_field": login.fields.username_field,
            ":password_field": login.fields.password_field,
            ":sec_fields": login.sec_fields,
            ":guid": login.record.id,
            ":time_created": login.record.time_created,
            ":times_used": login.record.times_used,
            ":time_last_used": login.record.time_last_used,
            ":time_password_changed": login.record.time_password_changed,
            // Local login specific stuff
            ":local_modified": util::system_time_ms_i64(local_login.local_modified),
            ":is_deleted": local_login.is_deleted,
            ":sync_status": local_login.sync_status as u8
        },
    ) {
        Ok(_) => log::info!("Imported {} successfully.", login.record.id),
        Err(e) => {
            log::warn!("Could not import {} ({}).", login.record.id, e);
            return Err(e.into());
        }
    };
    tx.commit()?;
    Ok(())
}

// This was copied from import_multiple in db.rs with a focus on LocalLogin
fn migrate_mirror_login(store: &LoginStore, mirror_login: &MirrorLogin) -> Result<()> {
    let new_db = store.db.lock().unwrap();
    let conn = new_db.conn();
    let tx = conn.unchecked_transaction()?;

    let sql = "INSERT OR IGNORE INTO loginsM (
        origin,
        httpRealm,
        formActionOrigin,
        usernameField,
        passwordField,
        timesUsed,
        secFields,
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
        :sec_fields,
        :guid,
        :time_created,
        :time_last_used,
        :time_password_changed,
        :server_modified,
        :is_overridden
    )";

    // As mirror syncs with the server, we should not attempt to apply fixups
    let login = &mirror_login.login;
    match conn.execute_named_cached(
        &sql,
        named_params! {
            ":origin": login.fields.origin,
            ":http_realm": login.fields.http_realm,
            ":form_action_origin": login.fields.form_action_origin,
            ":username_field": login.fields.username_field,
            ":password_field": login.fields.password_field,
            ":sec_fields": login.sec_fields,
            ":guid": login.record.id,
            ":time_created": login.record.time_created,
            ":times_used": login.record.times_used,
            ":time_last_used": login.record.time_last_used,
            ":time_password_changed": login.record.time_password_changed,
             // Mirror login specific stuff
             ":server_modified": mirror_login.server_modified.as_millis(),
             ":is_overridden": mirror_login.is_overridden
        },
    ) {
        Ok(_) => log::info!("Imported {} successfully.", login.record.id),
        Err(e) => {
            log::warn!("Could not import {} ({}).", login.record.id, e);
            return Err(e.into());
        }
    };
    tx.commit()?;
    Ok(())
}

// Convert rows from old schema to match new fields in the Login struct
fn get_login_from_row(row: &Row<'_>) -> Result<Login> {
    // We want to grab the "old" schema
    let guid: String = row.get("guid")?;
    let username: String = row.get("username").unwrap_or_default();
    let password: String = row.get("password").unwrap_or_default();
    // migrating hostname to the new column origin
    let origin: String = row.get("hostname").unwrap_or_default();
    let http_realm: Option<String> = row.get("httpRealm").unwrap_or_default();
    // migrating formSubmitURL to the new column action origin
    let form_action_origin: Option<String> = row.get("formSubmitURL").unwrap_or_default();
    let username_field: String = row.get("usernameField").unwrap_or_default();
    let password_field: String = row.get("passwordField").unwrap_or_default();
    let time_created: i64 = row.get("timeCreated").unwrap_or_default();
    let time_last_used: i64 = row.get("timeLastUsed").unwrap_or_default();
    let time_password_changed: i64 = row.get("timePasswordChanged").unwrap_or_default();
    let times_used: i64 = row.get("timesUsed").unwrap_or_default();

    let login = Login {
        record: RecordFields {
            id: guid,
            time_created,
            time_password_changed,
            time_last_used,
            times_used,
        },
        fields: LoginFields {
            origin,
            form_action_origin,
            http_realm,
            username_field,
            password_field,
        },
        sec_fields: SecureLoginFields { username, password },
    };
    Ok(login)
}

fn migrate_sync_metadata(cipher_conn: &Connection, store: &LoginStore) -> Result<MigrationMetrics> {
    let new_db = store.db.lock().unwrap();
    let conn = new_db.conn();
    let import_start = Instant::now();

    let mut select_stmt = cipher_conn.prepare("SELECT * FROM loginsSyncMeta")?;
    let mut rows = select_stmt.query(NO_PARAMS)?;

    let sql = "INSERT INTO loginsSyncMeta (key, value) VALUES (:key, :value)";

    let mut num_processed: u64 = 0;
    let mut num_failed_insert: u64 = 0;
    let mut insert_errors: Vec<String> = Vec::new();

    while let Some(row) = rows.next()? {
        num_processed += 1;
        let key: String = row.get("key")?;
        let value: String = row.get("value")?;

        match conn.execute_named_cached(&sql, named_params! { ":key": &key, ":value": &value }) {
            Ok(_) => log::info!("Imported {} successfully", key),
            Err(e) => {
                log::warn!("Could not import {}.", key);
                insert_errors.push(Error::from(e).label().into());
                num_failed_insert += 1;
            }
        }
    }
    Ok(MigrationMetrics {
        fixup_phase: MigrationPhaseMetrics::default(),
        insert_phase: MigrationPhaseMetrics::default(),
        errors: insert_errors,
        num_processed,
        num_failed: num_failed_insert,
        num_succeeded: num_processed - num_failed_insert,
        total_duration: import_start.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LoginDb;
    use crate::encryption::test_utils::{decrypt_struct, TEST_ENCRYPTION_KEY};
    use crate::schema;
    use crate::EncryptedLogin;
    use rusqlite::types::ValueRef;
    use std::path::PathBuf;
    use sync_guid::Guid;

    static TEST_SALT: &str = "01010101010101010101010101010101";

    fn open_old_db(db_path: impl AsRef<Path>, salt: Option<&str>) -> Connection {
        let mut db = Connection::open(db_path).unwrap();
        init_sqlcipher_db(&mut db, "old-key", salt).unwrap();
        sqlcipher_3_compat(&db).unwrap();
        db
    }

    fn create_old_db_with_test_data(db_path: impl AsRef<Path>, salt: Option<&str>, inserts: &str) {
        let mut db = open_old_db(db_path, salt);
        let tx = db.transaction().unwrap();
        schema::init(&tx).unwrap();

        // Note that we still abuse our current schema for this. As part of the migration away
        // from sqlcipher we renamed some columns, which we need to rename back.
        // (The alternative would be to clone the entire schema from the last sqlcipher version,
        // which isn't really any better than this, so meh.)
        const RENAMES: &str = "
            ALTER TABLE loginsL ADD COLUMN username;
            ALTER TABLE loginsL ADD COLUMN password;
            ALTER TABLE loginsM ADD COLUMN username;
            ALTER TABLE loginsM ADD COLUMN password;
            ALTER TABLE loginsL RENAME origin TO hostname;
            ALTER TABLE loginsL RENAME formActionOrigin TO formSubmitURL;
            ALTER TABLE loginsM RENAME origin TO hostname;
            ALTER TABLE loginsM RENAME formActionOrigin TO formSubmitURL;
        ";
        tx.execute_batch(&RENAMES).unwrap();
        tx.execute_batch(inserts).unwrap();
        tx.commit().unwrap();
    }

    fn create_old_db(db_path: impl AsRef<Path>, salt: Option<&str>) {
        const INSERTS: &str = r#"
            INSERT INTO loginsL(guid, username, password, hostname,
                httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
                timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
                VALUES
                ('a', 'test', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 2),
                ('b', 'test', 'password', 'https://www.example.com', 'https://www.example.com', 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 2),
                ('bad_sync_status', 'test', 'password', 'https://www.example2.com', 'https://www.example.com', 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 'invalid_status');

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

            -- Need to test migrating sync meta else we'll be resyncing everything
            INSERT INTO loginsSyncMeta (key, value) VALUES ("last_sync", "some_payload_data");
        "#;
        create_old_db_with_test_data(db_path, salt, INSERTS);
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

    fn create_local_login(
        login: EncryptedLogin,
        sync_status: SyncStatus,
        is_deleted: bool,
        local_modified: SystemTime,
    ) -> LocalLogin {
        LocalLogin {
            login,
            sync_status,
            is_deleted,
            local_modified,
        }
    }

    fn create_mirror_login(
        login: EncryptedLogin,
        is_overridden: bool,
        server_modified: ServerTimestamp,
    ) -> MirrorLogin {
        MirrorLogin {
            login,
            is_overridden,
            server_modified,
        }
    }

    fn check_migrated_data(db: &LoginDb) {
        let mut stmt = db
            .prepare("SELECT * FROM loginsL where guid = 'a'")
            .unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();
        let enc: SecureLoginFields =
            decrypt_struct(row.get_raw("secFields").as_str().unwrap().to_string());
        assert_eq!(enc.username, "test");
        assert_eq!(enc.password, "password");
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
        let enc: SecureLoginFields =
            decrypt_struct(row.get_raw("secFields").as_str().unwrap().to_string());
        assert_eq!(enc.username, "test");
        assert_eq!(enc.password, "password");
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
        // We should treat success per guid (localL + localM) rather than per record
        assert_eq!(metrics.num_processed, 3);
        assert_eq!(metrics.num_succeeded, 2);
        assert_eq!(metrics.num_failed, 1);
        assert_eq!(metrics.errors, ["InvalidLogin::EmptyOrigin"]);
    }

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
        assert_eq!(metrics.num_processed, 6);
        assert_eq!(metrics.num_succeeded, 5);
        assert_eq!(metrics.num_failed, 1);
        assert_eq!(metrics.errors.len(), 1);
    }

    #[test]
    #[ignore]
    fn test_migrate_broken_mirror_with_local() {
        let inserts = format!(
            r#"
            INSERT INTO loginsL(guid, username, password, hostname,
                httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
                timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
                VALUES
                ('b', 'test', 'password', 'https://www.example.com', 'https://www.example.com', 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, {status_new});

            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
                usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
                is_overridden, server_modified)
                VALUES
                ('b', 'test', 'password', 'https://www.example.com', 'Test Realm', NULL,
                '', '', "corrupt_time_created", "corrupt_time_last_used", "corrupt_time_changes", "corrupt_times_used", "corrupt_is_overridden", 1000);
        "#,
            status_new = SyncStatus::New as u8
        );
        let testpaths = TestPaths::new();
        create_old_db_with_test_data(testpaths.old_db.as_path(), None, &inserts);
        let _metrics = migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            None,
        )
        .unwrap();
        // This *should not* migrate the corrupt mirror record but should mark the sync_status as
        // SyncStatus::New - that will force us to grab a new mirror record from the server.
        todo!("check the above!");
    }

    #[test]
    #[ignore]
    fn test_migrate_broken_mirror_without_local() {
        // Just like the above, but *only* the mirror exists - so discarding it would be data-loss.
        // In that case we should take the record with the fixed up data.
        todo!("implement the above");
    }

    #[test]
    #[ignore]
    fn test_migrate_broken_local_without_mirror() {
        // Just like the above - corrupt data in the local record, but mirror is fine.
        // We should discard the local record keeping the mirror, but ensuring `is_overridden`
        // and SyncStatus are correct.
        todo!("implement the above");
    }

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

    fn gen_migrate_plan() -> MigrationPlan {
        let encdec = EncryptorDecryptor::new(&TEST_ENCRYPTION_KEY).unwrap();
        let mut migrate_plan = MigrationPlan::new();

        // Taken from db.rs
        let valid_login1 = Login {
            record: RecordFields {
                id: "a".to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                http_realm: None,
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test".into(),
                password: "test".into(),
            },
        };
        let valid_login_guid2: Guid = Guid::random();
        let valid_login2 = Login {
            record: RecordFields {
                id: valid_login_guid2.to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example2.com".into()),
                origin: "https://www.example2.com".into(),
                http_realm: None,
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test2".into(),
                password: "test2".into(),
            },
        };
        let valid_login_guid3: Guid = Guid::random();
        let valid_login3 = Login {
            record: RecordFields {
                id: valid_login_guid3.to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example3.com".into()),
                origin: "https://www.example3.com".into(),
                http_realm: None,
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test3".into(),
                password: "test3".into(),
            },
        };
        // local login + mirror login with override
        migrate_plan.logins.insert(
            valid_login1.guid().to_string(),
            MigrationLogin {
                local_login: Some(create_local_login(
                    valid_login1.clone().encrypt(&encdec).unwrap(),
                    SyncStatus::Synced,
                    false,
                    SystemTime::now(),
                )),
                mirror_login: Some(create_mirror_login(
                    valid_login1.clone().encrypt(&encdec).unwrap(),
                    true,
                    ServerTimestamp::from_millis(1000),
                )),
                status: MigrationStatus::Processing,
            },
        );

        // NO local login + mirror with override (should not be migrated)
        migrate_plan.logins.insert(
            valid_login2.guid().to_string(),
            MigrationLogin {
                local_login: None,
                mirror_login: Some(create_mirror_login(
                    valid_login2.clone().encrypt(&encdec).unwrap(),
                    true,
                    ServerTimestamp::from_millis(1000),
                )),
                status: MigrationStatus::Processing,
            },
        );

        // local +  NO mirror
        migrate_plan.logins.insert(
            valid_login2.guid().to_string(),
            MigrationLogin {
                local_login: Some(create_local_login(
                    valid_login3.clone().encrypt(&encdec).unwrap(),
                    SyncStatus::Synced,
                    false,
                    SystemTime::now(),
                )),
                mirror_login: None,
                status: MigrationStatus::Processing,
            },
        );

        migrate_plan
    }

    #[test]
    fn test_migrate_plan() {
        let testpaths = TestPaths::new();
        let store = LoginStore::new(testpaths.new_db.as_path()).unwrap();
        let encdec = EncryptorDecryptor::new(&TEST_ENCRYPTION_KEY).unwrap();
        let migration_plan = gen_migrate_plan();
        let metrics = migrate_logins(&migration_plan, &store, &encdec).unwrap();

        let db = LoginDb::open(testpaths.new_db.as_path()).unwrap();
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsL").unwrap(),
            2
        );
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsM").unwrap(),
            1
        );

        assert_eq!(metrics.num_processed, 2);
        assert_eq!(metrics.num_succeeded, 2);
        assert_eq!(metrics.total_duration > 0, true);
    }
}
