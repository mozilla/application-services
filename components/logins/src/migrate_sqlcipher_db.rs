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
use crate::LoginStore;
use crate::{Login, LoginFields, RecordFields, SecureLoginFields};
use rusqlite::{named_params, Connection, Row, NO_PARAMS};
use sql_support::ConnExt;
use std::collections::HashMap;
use std::ops::Add;
use std::path::Path;
use std::time::Instant;
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
    //guid: String,
    local_login: Option<LocalLogin>,
    mirror_login: Option<MirrorLogin>,
    status: MigrationStatus,
}

#[derive(Debug)]
enum MigrationStatus {
    Processing,
    Success,
    Fixed,
    Failed,
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

    // MIGRATION Plan
    // Step 1: We need to iterate through the old DB and throw it all into a struct that contains all the relevant information
    // Step 2: We need to perform options on the struct before throwing it into the new DB
    //      Step 2a: Find any mismatched data between local/mirro and either throw away or attempt to reconcile
    //      Step 2b: Find any logins we can fixup [LOCAL ONLY] and perform the proper operations
    // Step 3: Insert the struct data into the new DB
    //      Step 3a: Capture the right metrics

    // Step 1
    let mut migration_plan: MigrationPlan = generate_plan_from_db(&cipher_conn, &encdec)?;

    // Step 2: Handle mismatch records, edge cases before fixup
    migration_plan = migration_plan.fix_mismatched_records()?;

    // Step 3: Apply fixups to LOCAL logins

    // TODO Step 4: Insert (raw SQL) into new DB
    migrate_logins(&migration_plan, &new_db_store)?;

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
                    local_modified: util::system_time_millis_from_row(row, "local_modified")?,
                    is_deleted: row.get("is_deleted")?,
                    sync_status: SyncStatus::from_u8(row.get("sync_status")?)?,
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
                    server_modified: ServerTimestamp(row.get::<_, i64>("server_modified")?),
                    is_overridden: row.get("is_overridden")?,
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

fn migrate_logins(migration_plan: &MigrationPlan, store: &LoginStore) -> Result<()> {
    for (guid, login) in &migration_plan.logins {
        // Migrate local login first
        if let Some(local_login) = &login.local_login {
            match migrate_local_login(store, local_login) {
                Ok(_) => {
                    println!("Successfully migrated local record ");
                    if let Some(mirror_login) = &login.mirror_login {
                        // If successful, then migrate mirror
                        migrate_mirror_login(&store, &mirror_login)?;
                        println!("Successfully migrated mirror record");
                    }
                }
                Err(e) => {
                    // If not successful on local, but we have a mirror???
                }
            }
        // If we just have mirror, try to import
        } else {
            if let Some(mirror_login) = &login.mirror_login {
                match migrate_mirror_login(&*store, &mirror_login) {
                    Ok(_) => {
                        println!("Successfully migrated mirror record");
                    }
                    Err(e) => {}
                }
            }
        }
    }

    Ok(())
}

// This was copied from import_multiple in db.rs with a focus on LocalLogin
fn migrate_local_login(store: &LoginStore, local_login: &LocalLogin) -> Result<()> {
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

    let login = &local_login.login;

    // TODO: Figure out how to fixup fields
    // let maybe_fixed_login = login.maybe_fixup().and_then(|fixed| {
    //     match &fixed {
    //         None => new_db.check_for_dupes(&login)?,
    //         Some(l) => new_db.check_for_dupes(&l)?,
    //     };
    //     Ok(fixed)
    // });
    // match maybe_fixed_login {
    //     Ok(None) => {} // The provided login was fine all along
    //     Ok(Some(l)) => {
    //         // We made a new, fixed-up Login.
    //         login = l;
    //     }
    //     Err(e) => {
    //         log::warn!("Skipping login {} as it is invalid ({}).", login.guid(), e);
    //         // fixup_errors.push(e.label().into());
    //         // num_failed_fixup += 1;
    //         // continue;
    //     }
    // };

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
            //insert_errors.push(Error::from(e).label().into());
            //num_failed_insert += 1;
        }
    };
    tx.commit()?;

    // log::info!(
    //     "Finished importing logins with the following metrics: {:#?}",
    //     metrics
    // );
    // Ok(metrics)
    Ok(())
}

// This was copied from import_multiple in db.rs with a focus on LocalLogin
fn migrate_mirror_login(store: &LoginStore, mirror_login: &MirrorLogin) -> Result<()> {
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

    // Need to revisit this clone
    let login = &mirror_login.login;

    // let maybe_fixed_login = login.maybe_fixup().and_then(|fixed| {
    //     match &fixed {
    //         None => new_db.check_for_dupes(&login)?,
    //         Some(l) => new_db.check_for_dupes(&l)?,
    //     };
    //     Ok(fixed)
    // });
    // match maybe_fixed_login {
    //     Ok(None) => {} // The provided login was fine all along
    //     Ok(Some(l)) => {
    //         // We made a new, fixed-up Login.
    //         login = l;
    //     }
    //     Err(e) => {
    //         log::warn!("Skipping login {} as it is invalid ({}).", login.guid(), e);
    //         // fixup_errors.push(e.label().into());
    //         // num_failed_fixup += 1;
    //         // continue;
    //     }
    // };

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
            //insert_errors.push(Error::from(e).label().into());
            //num_failed_insert += 1;
        }
    };
    tx.commit()?;

    // log::info!(
    //     "Finished importing logins with the following metrics: {:#?}",
    //     metrics
    // );
    // Ok(metrics)
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
        assert_eq!(metrics.num_processed, 6);
        assert_eq!(metrics.num_succeeded, 5);
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
        assert_eq!(metrics.num_processed, 6);
        assert_eq!(metrics.num_succeeded, 5);
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
