/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Code to migrate from an sqlcipher DB to a plaintext DB

use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::sync::{LocalLogin, MirrorLogin, SyncStatus};
use crate::util;
use crate::ValidateAndFixup;
use crate::{
    EncryptedLogin, Login, LoginDb, LoginEntry, LoginFields, LoginStore, RecordFields,
    SecureLoginFields,
};
use rusqlite::{named_params, types::Value, Connection, Row};
use sql_support::ConnExt;
use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;
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
}

#[derive(Debug)]
struct MigrationLogin {
    local_login: Option<LocalLogin>,
    mirror_login: Option<MirrorLogin>,
    migration_op: MigrationOp,
}

#[derive(Debug, PartialEq)]
enum MigrationOp {
    Normal,        // Migrate as normal
    FixedLocal,    // Local was fixed up, any mirror should be overridden
    MirrorToLocal, // Local is irreparable, mirror goes to local
    Skip(String),  // Fatal issue, don't migrate
}

// migration for consumers to migrate from their SQLCipher DB
pub fn migrate_logins(
    path: impl AsRef<Path>,
    new_encryption_key: &str,
    sqlcipher_path: impl AsRef<Path>,
    sqlcipher_key: &str,
    // The salt arg is for iOS where the salt is stored externally.
    salt: Option<String>,
) -> ApiResult<()> {
    handle_error! {
        let path = path.as_ref();
        let sqlcipher_path = sqlcipher_path.as_ref();

        // If the sqlcipher db doesn't exist we can't do anything.
        if !sqlcipher_path.exists() {
            return Err(LoginsError::InvalidDatabaseFile(
                sqlcipher_path.to_string_lossy().to_string(),
            ));
        }

        // If the target does exist we fail as we don't want to migrate twice.
        if path.exists() {
            return Err(LoginsError::MigrationError(
                "target database already exists".to_string(),
            ));
        }
        migrate_sqlcipher_db_to_plaintext(
            &sqlcipher_path,
            &path,
            sqlcipher_key,
            new_encryption_key,
            salt.as_ref(),
        )
    }
}

fn migrate_sqlcipher_db_to_plaintext(
    old_db_path: impl AsRef<Path>,
    new_db_path: impl AsRef<Path>,
    old_encryption_key: &str,
    new_encryption_key: &str,
    salt: Option<&String>,
) -> Result<()> {
    let mut db = Connection::open(old_db_path)?;
    init_sqlcipher_db(&mut db, old_encryption_key, salt)?;

    // Init the new plaintext db as we would a regular client
    let new_db_store = LoginStore::new_from_db(LoginDb::open(new_db_path)?);
    migrate_from_sqlcipher_db(&mut db, new_db_store, new_encryption_key)
}

fn init_sqlcipher_db(
    db: &mut Connection,
    encryption_key: &str,
    salt: Option<&String>,
) -> Result<()> {
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
fn migrate_from_sqlcipher_db(
    cipher_conn: &mut Connection,
    new_db_store: LoginStore,
    encryption_key: &str,
) -> Result<()> {
    // encrypt the username/password data
    let encdec = EncryptorDecryptor::new(encryption_key)?;

    let migration_plan: MigrationPlan = generate_plan_from_db(cipher_conn, &encdec)?;
    insert_logins(&migration_plan, &new_db_store, &encdec)?;
    migrate_sync_metadata(cipher_conn, &new_db_store)
}

fn generate_plan_from_db(
    cipher_conn: &Connection,
    encdec: &EncryptorDecryptor,
) -> Result<MigrationPlan> {
    let mut migration_plan = MigrationPlan::new();

    // Process local logins and add to MigrationPlan
    let mut local_stmt = cipher_conn.prepare("SELECT * FROM loginsL")?;
    let mut local_rows = local_stmt.query([])?;
    while let Some(row) = local_rows.next()? {
        match get_login_from_row(row) {
            Ok(login) => {
                let l_login = LocalLogin {
                    login: login.encrypt(encdec)?,
                    local_modified: util::system_time_millis_from_row(row, "local_modified")
                        .unwrap_or_else(|_| SystemTime::now()),
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
                        local_login: Some(l_login),
                        mirror_login: None,
                        migration_op: MigrationOp::Normal,
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
    let mut mirror_rows = mirror_stmt.query([])?;
    while let Some(row) = mirror_rows.next()? {
        match get_login_from_row(row) {
            Ok(login) => {
                let m_login = MirrorLogin {
                    login: login.encrypt(encdec)?,
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
                        local_login: None,
                        mirror_login: Some(m_login),
                        migration_op: MigrationOp::Normal,
                    });
            }
            Err(e) => {
                // We should probably just skip if we can't successfully fetch the row
                log::warn!("Error getting record from DB: {:?}", e);
            }
        }
    }
    migration_plan = apply_migration_fixups(migration_plan, encdec)?;
    Ok(migration_plan)
}

fn apply_migration_fixups(
    migration_plan: MigrationPlan,
    encdec: &EncryptorDecryptor,
) -> Result<MigrationPlan> {
    // This list contains the delta of any changes that we found in the MigrationPlan we plan to put
    // in the new db and will replace any MigrationLogin with a matching guid with the fixed up version
    let mut logins_to_override: HashMap<String, MigrationLogin> = HashMap::new();

    for (guid, migration_login) in &migration_plan.logins {
        match (
            migration_login.local_login.as_ref(),
            migration_login.mirror_login.as_ref(),
        ) {
            (Some(local_login), Some(mirror_login)) => {
                // We have both a local and mirror
                // attempt to fixup local and override mirror
                match fixup_local_login(local_login, encdec) {
                    Ok(Some(new_entry)) => {
                        logins_to_override.insert(
                            guid.to_string(),
                            MigrationLogin {
                                mirror_login: Some(MirrorLogin {
                                    is_overridden: true,
                                    ..mirror_login.clone()
                                }),
                                local_login: Some(LocalLogin {
                                    login: EncryptedLogin::from_fixed(
                                        local_login.login.record.clone(),
                                        new_entry,
                                        encdec,
                                    )?,
                                    sync_status: SyncStatus::Changed,
                                    local_modified: local_login.local_modified,
                                    is_deleted: local_login.is_deleted,
                                }),
                                migration_op: MigrationOp::FixedLocal,
                            },
                        );
                    }
                    Ok(None) => {}
                    Err(_) => {
                        // Could not fixup local, dump it and set mirror to not be overidden
                        logins_to_override.insert(
                            guid.to_string(),
                            MigrationLogin {
                                mirror_login: Some(MirrorLogin {
                                    is_overridden: false,
                                    ..mirror_login.clone()
                                }),
                                local_login: None,
                                migration_op: MigrationOp::MirrorToLocal,
                            },
                        );
                    }
                };
            }
            (Some(local_login), None) => {
                // Only local
                match fixup_local_login(local_login, encdec) {
                    Ok(Some(new_entry)) => {
                        logins_to_override.insert(
                            guid.to_string(),
                            MigrationLogin {
                                mirror_login: None,
                                local_login: Some(LocalLogin {
                                    //login,
                                    login: EncryptedLogin::from_fixed(
                                        local_login.login.record.clone(),
                                        new_entry,
                                        encdec,
                                    )?,
                                    sync_status: SyncStatus::New,
                                    local_modified: SystemTime::now(),
                                    is_deleted: local_login.is_deleted,
                                }),
                                migration_op: MigrationOp::FixedLocal,
                            },
                        );
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::warn!(
                            "Guid {}: Could not fix up local and no mirror, data loss - {}",
                            guid,
                            e
                        );
                        logins_to_override.insert(
                            guid.to_string(),
                            MigrationLogin {
                                local_login: None,
                                mirror_login: None,
                                migration_op: MigrationOp::Skip(e.to_string()),
                            },
                        );
                    }
                };
            }
            (None, Some(mirror_login)) => {
                if mirror_login.is_overridden {
                    logins_to_override.insert(
                        guid.to_string(),
                        MigrationLogin {
                            mirror_login: Some(MirrorLogin {
                                is_overridden: false,
                                ..mirror_login.clone()
                            }),
                            local_login: None,
                            migration_op: MigrationOp::Normal,
                        },
                    );
                }
                let dec_login = mirror_login.login.clone().decrypt(encdec)?;
                // If we somehow ended up with a invalid mirror and no local, try to fixup and move into local
                match dec_login.entry().maybe_fixup() {
                    Ok(Some(new_entry)) => {
                        logins_to_override.insert(
                            guid.to_string(),
                            MigrationLogin {
                                mirror_login: None,
                                // Note: mirror is becoming a local login here
                                local_login: Some(LocalLogin {
                                    login: EncryptedLogin::from_fixed(
                                        mirror_login.login.record.clone(),
                                        new_entry,
                                        encdec,
                                    )?,
                                    sync_status: SyncStatus::New,
                                    local_modified: SystemTime::now(),
                                    is_deleted: false,
                                }),
                                migration_op: MigrationOp::MirrorToLocal,
                            },
                        );
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::warn!(
                            "Guid {}: Could not fix up mirror and no local, data loss - {}",
                            guid,
                            e
                        );
                        logins_to_override.insert(
                            guid.to_string(),
                            MigrationLogin {
                                local_login: None,
                                mirror_login: None,
                                migration_op: MigrationOp::Skip(e.to_string()),
                            },
                        );
                    }
                }
            }
            (None, None) => unreachable!("we never create this"),
        };
    }
    // override any logins that are in new hashmap
    Ok(MigrationPlan {
        logins: migration_plan
            .logins
            .into_iter()
            .chain(logins_to_override)
            .collect(),
    })
}

fn fixup_local_login(
    local_login: &LocalLogin,
    encdec: &EncryptorDecryptor,
) -> Result<Option<LoginEntry>> {
    if local_login.is_deleted {
        // Delete logins don't have valid data, so don't try to run them through maybe_fixup()
        Ok(None)
    } else {
        local_login
            .login
            .clone()
            .decrypt(encdec)?
            .entry()
            .maybe_fixup()
    }
}

fn insert_logins(
    migration_plan: &MigrationPlan,
    store: &LoginStore,
    encdec: &EncryptorDecryptor,
) -> Result<()> {
    let new_db = store.db.lock();
    let conn = new_db.conn();
    let tx = conn.unchecked_transaction()?;

    for login in migration_plan.logins.values() {
        // Could not easily use an equality here due to the inner value
        if let MigrationOp::Skip(err) = &login.migration_op {
            log::info!("skipping migration of login: {err}");
            continue;
        };
        // // Migrate local login first
        if let Some(local_login) = &login.local_login {
            match insert_local_login(conn, &new_db, encdec, local_login) {
                Ok(_) => {
                    if let Some(mirror_login) = &login.mirror_login {
                        // If successful, then migrate mirror also
                        if let Err(e) = insert_mirror_login(conn, mirror_login) {
                            log::info!("failed to insert a migrated login mirror: {e}");
                        }
                    }
                }
                Err(e) => {
                    log::info!("failed to insert a migrated login record: {e}");
                    // Weren't successful with local login, if we have a mirror we should
                    // attempt to migrate it and flip the `is_overridden` to false
                    if let Some(mirror_login) = &login.mirror_login {
                        if let Err(err) = insert_mirror_login(
                            conn,
                            &MirrorLogin {
                                is_overridden: false,
                                ..mirror_login.clone()
                            },
                        ) {
                            log::info!("failed to modify a migrated login mirror: {err}");
                        }
                    }
                }
            }
        // If we just have mirror, import as normal
        } else if let Some(mirror_login) = &login.mirror_login {
            if let Err(err) = insert_mirror_login(conn, mirror_login) {
                log::info!("failed to insert a lonely login mirror: {err}");
            }
        }
    }
    tx.commit()?;
    Ok(())
}

fn insert_local_login(
    conn: &Connection,
    new_db: &LoginDb,
    encdec: &EncryptorDecryptor,
    local_login: &LocalLogin,
) -> Result<()> {
    let sql = "INSERT INTO loginsL (
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
    let dec_login = &local_login.login.clone().decrypt(encdec)?;

    // Check for existing duplicate logins
    //
    // Skip the check for deleted logins, which have issues with the dupe checking logic, since
    // http_realm and form_action_origin may be unset.  This doesn't currently happen on mobile,
    // but it might on desktop (see #4573)
    if !local_login.is_deleted {
        if let Err(e) = new_db.check_for_dupes(&login.guid(), &dec_login.entry(), encdec) {
            log::warn!("Duplicate {} ({}).", login.record.id, e);
            return Ok(());
        };
    }
    match conn.execute_cached(
        sql,
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
    Ok(())
}

fn insert_mirror_login(conn: &Connection, mirror_login: &MirrorLogin) -> Result<()> {
    let sql = "INSERT INTO loginsM (
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
    match conn.execute_cached(
        sql,
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

fn migrate_sync_metadata(cipher_conn: &Connection, store: &LoginStore) -> Result<()> {
    let new_db = store.db.lock();
    let conn = new_db.conn();

    let mut select_stmt = cipher_conn.prepare("SELECT key, value FROM loginsSyncMeta")?;
    let mut rows = select_stmt.query([])?;

    let sql = "INSERT INTO loginsSyncMeta (key, value) VALUES (:key, :value)";

    while let Some(row) = rows.next()? {
        let key: String = row.get("key")?;
        let value: Value = row.get("value")?;

        match conn.execute_cached(sql, named_params! { ":key": &key, ":value": &value }) {
            Ok(_) => log::info!("Imported {key} successfully"),
            Err(e) => {
                log::warn!("Could not import {key} - {e}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LoginDb;
    use crate::encryption::test_utils::{decrypt_struct, TEST_ENCRYPTION_KEY};
    use crate::schema;
    use crate::sync::LocalLogin;
    use crate::EncryptedLogin;
    use rusqlite::types::ValueRef;
    use std::path::PathBuf;
    use sync_guid::Guid;

    static TEST_SALT: &str = "01010101010101010101010101010101";

    fn open_old_db(db_path: impl AsRef<Path>, salt: Option<&String>) -> Connection {
        let mut db = Connection::open(db_path).unwrap();
        init_sqlcipher_db(&mut db, "old-key", salt).unwrap();
        sqlcipher_3_compat(&db).unwrap();
        db
    }

    fn create_old_db_with_test_data(
        db_path: impl AsRef<Path>,
        salt: Option<&String>,
        inserts: &str,
    ) {
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
        tx.execute_batch(RENAMES).unwrap();
        tx.execute_batch(inserts).unwrap();
        tx.commit().unwrap();
    }

    fn create_old_db(db_path: impl AsRef<Path>, salt: Option<&String>) {
        const INSERTS: &str = r#"
            INSERT INTO loginsL(guid, username, password, hostname,
                httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
                timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
                VALUES
                ('a', 'test', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 0),
                ('b', 'test', 'password', 'https://www.example1.com', 'https://www.example1.com', 'https://www.example1.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 2),
                ('bad_sync_status', 'test', 'password', 'https://www.example2.com', 'https://www.example.com', 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 'invalid_status'),
                ('d', 'test', 'password', '', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 1, 0, 1000);

            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
                usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
                is_overridden, server_modified)
                VALUES
                ('b', 'test', 'password', 'https://www.example1.com', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 0, 1000),
                ('c', 'test', 'password', 'http://example.com:1234/', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 0, 1000),
                ('e', 'test', 'password', 'www.mirror_only.com', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 1, 1000);

            -- Need to test migrating sync meta else we'll be resyncing everything
            INSERT INTO loginsSyncMeta (key, value)
                VALUES
                ("password_sync_id", "test-sync-id"),
                ("last_sync_time", 1234);
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
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        let enc: SecureLoginFields = decrypt_struct(
            row.get_ref_unwrap("secFields")
                .as_str()
                .unwrap()
                .to_string(),
        );
        assert_eq!(enc.username, "test");
        assert_eq!(enc.password, "password");
        assert_eq!(
            row.get_ref_unwrap("origin").as_str().unwrap(),
            "https://www.example.com"
        );
        assert_eq!(row.get_ref_unwrap("httpRealm"), ValueRef::Null);
        assert_eq!(
            row.get_ref_unwrap("formActionOrigin").as_str().unwrap(),
            "https://www.example.com"
        );
        assert_eq!(
            row.get_ref_unwrap("usernameField").as_str().unwrap(),
            "username"
        );
        assert_eq!(
            row.get_ref_unwrap("passwordField").as_str().unwrap(),
            "password"
        );
        assert_eq!(row.get_ref_unwrap("timeCreated").as_i64().unwrap(), 1000);
        assert_eq!(row.get_ref_unwrap("timeLastUsed").as_i64().unwrap(), 1000);
        assert_eq!(
            row.get_ref_unwrap("timePasswordChanged").as_i64().unwrap(),
            1
        );
        assert_eq!(row.get_ref_unwrap("timesUsed").as_i64().unwrap(), 10);
        assert_eq!(row.get_ref_unwrap("is_deleted").as_i64().unwrap(), 0);
        assert_eq!(row.get_ref_unwrap("sync_status").as_i64().unwrap(), 0);

        let mut stmt = db
            .prepare("SELECT * FROM loginsM WHERE guid = 'b'")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        let enc: SecureLoginFields = decrypt_struct(
            row.get_ref_unwrap("secFields")
                .as_str()
                .unwrap()
                .to_string(),
        );
        assert_eq!(enc.username, "test");
        assert_eq!(enc.password, "password");
        assert_eq!(
            row.get_ref_unwrap("origin").as_str().unwrap(),
            "https://www.example1.com"
        );
        assert_eq!(
            row.get_ref_unwrap("httpRealm").as_str().unwrap(),
            "Test Realm"
        );
        assert_eq!(row.get_ref_unwrap("formActionOrigin"), ValueRef::Null);
        assert_eq!(row.get_ref_unwrap("usernameField").as_str().unwrap(), "");
        assert_eq!(row.get_ref_unwrap("passwordField").as_str().unwrap(), "");
        assert_eq!(row.get_ref_unwrap("timeCreated").as_i64().unwrap(), 1000);
        assert_eq!(row.get_ref_unwrap("timeLastUsed").as_i64().unwrap(), 1000);
        assert_eq!(
            row.get_ref_unwrap("timePasswordChanged").as_i64().unwrap(),
            1
        );
        assert_eq!(row.get_ref_unwrap("timesUsed").as_i64().unwrap(), 10);

        assert_eq!(row.get_ref_unwrap("is_overridden").as_i64().unwrap(), 1);
        assert_eq!(
            row.get_ref_unwrap("server_modified").as_i64().unwrap(),
            1000
        );

        // Ensure loginsSyncMeta migrated correctly
        assert_eq!(
            db.query_one::<String>("SELECT value FROM loginsSyncMeta WHERE key='password_sync_id'")
                .unwrap(),
            "test-sync-id"
        );
        assert_eq!(
            db.query_one::<i32>("SELECT value FROM loginsSyncMeta WHERE key='last_sync_time'")
                .unwrap(),
            1234
        );

        // The schema version should reset to 1 after the migration
        assert_eq!(db.query_one::<i64>("PRAGMA user_version").unwrap(), 1);
    }

    #[test]
    fn test_migrate_data() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), None);
        migrate_sqlcipher_db_to_plaintext(
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
    }

    #[test]
    fn test_migration_errors() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), None);
        let old_db = open_old_db(testpaths.old_db.as_path(), None);
        old_db
            .execute("UPDATE loginsM SET username = NULL WHERE guid='e'", [])
            .unwrap();
        drop(old_db);

        migrate_sqlcipher_db_to_plaintext(
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
            4
        );
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsM").unwrap(),
            1
        );
    }

    #[test]
    // This *should not* migrate the corrupt mirror record but should mark the sync_status as
    // SyncStatus::Changed - that will force us to grab a new mirror record from the server.
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
        migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            None,
        )
        .unwrap();

        let db = LoginDb::open(testpaths.new_db).unwrap();
        let mut stmt = db
            .prepare("SELECT * FROM loginsL WHERE guid = 'b'")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        assert_eq!(
            row.get_ref_unwrap("sync_status").as_i64().unwrap(),
            1 // = SyncStatus::Changed
        );

        let mut stmt = db
            .prepare("SELECT * FROM loginsM WHERE guid = 'b'")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        assert_eq!(row.get_ref_unwrap("is_overridden").as_i64().unwrap(), 1);
    }

    #[test]
    // Just like the above, but *only* the mirror exists - so discarding it would be data-loss.
    // In that case we should take the record with the fixed up data and insert into loginsL
    fn test_migrate_broken_mirror_without_local() {
        let inserts = r#"
            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
                usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
                is_overridden, server_modified)
                VALUES
                ('b', 'test', 'password', 'https://www.example.com', 'Test Realm', 'https://www.example.com',
                '', '', "corrupt_time_created", "corrupt_time_last_used", "corrupt_time_changes", "corrupt_times_used", "corrupt_is_overridden", 1000);
        "#;
        let testpaths = TestPaths::new();
        create_old_db_with_test_data(testpaths.old_db.as_path(), None, inserts);
        migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            None,
        )
        .unwrap();

        let db = LoginDb::open(testpaths.new_db).unwrap();
        let mut stmt = db
            .prepare("SELECT * FROM loginsL WHERE guid = 'b'")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        let enc: SecureLoginFields = decrypt_struct(
            row.get_ref_unwrap("secFields")
                .as_str()
                .unwrap()
                .to_string(),
        );
        assert_eq!(enc.username, "test");
        assert_eq!(enc.password, "password");
        assert_eq!(
            row.get_ref_unwrap("origin").as_str().unwrap(),
            "https://www.example.com"
        );

        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsL").unwrap(),
            1
        );
    }

    #[test]
    fn test_migrate_broken_local_without_mirror() {
        // Just like the above - corrupt data in the local record, but mirror is fine.
        // We should discard the local record keeping the mirror, but ensuring `is_overridden`
        // and SyncStatus are correct.
        let inserts = r#"
            INSERT INTO loginsL(guid, username, password, hostname,
                httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
                timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
                VALUES
                ('b', 'test', 'password', '', 'https://www.example.com', 'https://www.example.com',
                'username', 'password', 'corrupt', 1000, 1, 10, 1000, 0, 2);

            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
                usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
                is_overridden, server_modified)
                VALUES
                ('b', 'test', 'password', 'https://www.example.com', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 1, 1000);
        "#;
        let testpaths = TestPaths::new();
        create_old_db_with_test_data(testpaths.old_db.as_path(), None, inserts);
        migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            None,
        )
        .unwrap();

        // // Just like the above, but *only* the mirror exists - so discarding it would be data-loss.
        // // In that case we should take the record with the fixed up data.
        let db = LoginDb::open(testpaths.new_db).unwrap();
        let mut stmt = db
            .prepare("SELECT * FROM loginsM WHERE guid = 'b'")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        let enc: SecureLoginFields = decrypt_struct(
            row.get_ref("secFields")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
        );
        assert_eq!(enc.username, "test");
        assert_eq!(enc.password, "password");
        assert_eq!(row.get_ref_unwrap("is_overridden").as_i64().unwrap(), 0);
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsL").unwrap(),
            0
        );
    }

    #[test]
    fn test_migrate_with_manual_salt() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), Some(&String::from(TEST_SALT)));
        migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            &TEST_ENCRYPTION_KEY,
            Some(&String::from(TEST_SALT)),
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
                    valid_login1.encrypt(&encdec).unwrap(),
                    true,
                    ServerTimestamp::from_millis(1000),
                )),
                migration_op: MigrationOp::Normal,
            },
        );

        // NO local login + mirror with override
        migrate_plan.logins.insert(
            valid_login2.guid().to_string(),
            MigrationLogin {
                local_login: None,
                mirror_login: Some(create_mirror_login(
                    valid_login2.clone().encrypt(&encdec).unwrap(),
                    true,
                    ServerTimestamp::from_millis(1000),
                )),
                migration_op: MigrationOp::MirrorToLocal,
            },
        );

        // local +  NO mirror
        migrate_plan.logins.insert(
            valid_login2.guid().to_string(),
            MigrationLogin {
                local_login: Some(create_local_login(
                    valid_login3.encrypt(&encdec).unwrap(),
                    SyncStatus::Synced,
                    false,
                    SystemTime::now(),
                )),
                mirror_login: None,
                migration_op: MigrationOp::Normal,
            },
        );

        migrate_plan
    }

    #[test]
    fn test_migrate_plan() {
        let testpaths = TestPaths::new();
        let store = LoginStore::new(testpaths.new_db.as_path()).unwrap();
        let migration_plan = gen_migrate_plan();
        let encdec = EncryptorDecryptor::new(&TEST_ENCRYPTION_KEY).unwrap();
        insert_logins(&migration_plan, &store, &encdec).unwrap();

        let db = LoginDb::open(testpaths.new_db.as_path()).unwrap();
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsL").unwrap(),
            2
        );
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsM").unwrap(),
            1
        );
    }

    #[test]
    fn test_invalid_sqlcipher_path() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), Some(&String::from(TEST_SALT)));

        assert!(matches!(
            migrate_logins(
                testpaths.new_db.as_path(),
                &TEST_ENCRYPTION_KEY,
                "/some/path/to/db.db",
                "old-key",
                None,
            )
            .err()
            .unwrap(),
            LoginsStorageError::UnexpectedLoginsStorageError(s) if s == "Invalid database file: /some/path/to/db.db",
        ))
    }

    #[test]
    fn test_existing_new_db() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), Some(&String::from(TEST_SALT)));

        // We want a new db to exist already
        LoginStore::new(testpaths.new_db.as_path()).unwrap();

        assert!(matches!(
            migrate_logins(
                testpaths.new_db.as_path(),
                &TEST_ENCRYPTION_KEY,
                testpaths.old_db.as_path(),
                "old-key",
                None,
            )
            .err()
            .unwrap(),
            LoginsStorageError::UnexpectedLoginsStorageError(s) if s == "Migration Error: target database already exists",
        ));
    }

    #[test]
    fn test_no_dupes_after_merge() {
        let inserts = r#"
            INSERT INTO loginsL(guid, username, password, hostname,
                httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
                timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
                VALUES
                ('a', 'test', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 0),
                ('b', '', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 0),
                ('c', '', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 0),
                ('d', '', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 0),
                ('e', '', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
                'username', 'password', 1000, 1000, 1, 10, 1000, 0, 0);

            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
                usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
                is_overridden, server_modified)
                VALUES
                ('b', 'test', 'password', 'https://www.example.com', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 1, 1000);
        "#;
        let testpaths = TestPaths::new();
        create_old_db_with_test_data(testpaths.old_db.as_path(), None, inserts);
        migrate_logins(
            testpaths.new_db.as_path(),
            &TEST_ENCRYPTION_KEY,
            testpaths.old_db.as_path(),
            "old-key",
            None,
        )
        .unwrap();

        let db = LoginDb::open(testpaths.new_db).unwrap();
        // Should only be 2 logins in loginsL (blank username and test)
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsL").unwrap(),
            2
        );
    }

    #[test]
    fn test_migrate_tombstone() {
        // Test migrating a tombstone (a deleted local record)
        let inserts = r#"
            INSERT INTO loginsL(guid, local_modified, is_deleted, sync_status, hostname, timeCreated, timePasswordChanged, secFields)
                VALUES
                ("A", 2000, 1, 1, '', 1000, 2000, '');
            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
                usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
                is_overridden, server_modified)
                VALUES
                ('A', 'test', 'password', 'https://www.example.com', 'Test Realm', NULL,
                '', '', 1000, 1000, 1, 10, 1, 1000);
        "#;
        let testpaths = TestPaths::new();
        create_old_db_with_test_data(testpaths.old_db.as_path(), None, inserts);
        migrate_logins(
            testpaths.new_db.as_path(),
            &TEST_ENCRYPTION_KEY,
            testpaths.old_db.as_path(),
            "old-key",
            None,
        )
        .unwrap();

        let db = LoginDb::open(testpaths.new_db).unwrap();
        let login = db
            .query_row_and_then(
                "SELECT * from loginsL WHERE Guid='A'",
                [],
                LocalLogin::from_row,
            )
            .unwrap();
        assert_eq!(login.login.fields.origin, "");
        assert!(login.is_deleted);
        assert_eq!(util::system_time_ms_i64(login.local_modified), 2000);
        assert_eq!(login.sync_status, SyncStatus::Changed);
    }
}
