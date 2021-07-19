/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Code to migrate from an sqlcipher DB to a plaintext DB

use crate::db::MigrationMetrics;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::Login;
use crate::LoginStore;
use lazy_static::lazy_static;
use rusqlite::{Connection, NO_PARAMS};
use sql_support::ConnExt;
use std::path::Path;
use std::time::Instant;

// Keep it around to be easy to reference the old schema
pub const SQL_CIPHER_COLS: &str = "
    guid,
    username,
    password,
    hostname,
    httpRealm,
    formSubmitURL,
    usernameField,
    passwordField,
    timeCreated,
    timeLastUsed,
    timePasswordChanged,
    timesUsed
";

lazy_static! {
    pub static ref GET_BY_GUID_SQL: String = format!(
        "SELECT {common_cols}
         FROM loginsL
         WHERE is_deleted = 0

         UNION ALL
    
         SELECT {common_cols}
         FROM loginsM
         WHERE is_overridden IS NOT 1
        ",
        common_cols = SQL_CIPHER_COLS,
    );
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
    let start_time = Instant::now();

    let mut metrics: MigrationMetrics = MigrationMetrics::default();

    // Select From both LoginsL and LoginsM with a union to ensure we're covering our
    // migration cases (one table has but not the other, etc)
    let mut select_stmt = cipher_conn.prepare(&GET_BY_GUID_SQL)?;

    // Use raw rows to avoid extra copying since we're looping over an entire table
    let mut rows = select_stmt.query(NO_PARAMS)?;
    while let Some(row) = rows.next()? {
        metrics.num_processed += 1;
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

        // TODO: Discuss
        // Need to handle in loginsL: local_modified, is_deleted
        // Feels like we potentially shouldn't migrate these fields?

        // Need to handle in loginsM: is_overridden, server_modified
        // Similar to the other: I only see server_modified potentially being needed

        // encrypt the username/password data
        let encryptor = EncryptorDecryptor::new(encryption_key)?;
        let login: Login = Login {
            id: guid.to_string(),
            username_enc: encryptor.encrypt(&username)?,
            password_enc: encryptor.encrypt(&password)?,
            origin,
            http_realm,
            form_action_origin,
            username_field: username_field.unwrap_or_default(),
            password_field: password_field.unwrap_or_default(),
            // TO_DO: Do we need to convert from microsecond timestamps to milliseconds??
            time_created,
            time_last_used,
            time_password_changed,
            times_used,
        };

        // Leveraging the add_or_update to get free fixup
        match new_db_store.add_or_update(login) {
            Ok(_) => {
                metrics.num_succeeded += 1;
            }
            Err(e) => {
                metrics.num_failed += 1;
                metrics.errors.push(e.to_string());
            }
        }
    }

    metrics.total_duration = start_time.elapsed().as_millis() as u64;
    Ok(metrics)
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
        tx.execute_batch(
            "
            ALTER TABLE loginsL RENAME usernameEnc to username;
            ALTER TABLE loginsL RENAME passwordEnc to password;
            ALTER TABLE loginsM RENAME usernameEnc to username;
            ALTER TABLE loginsM RENAME passwordEnc to password;
            ALTER TABLE loginsL RENAME origin to hostname;
            ALTER TABLE loginsL RENAME formActionOrigin to formSubmitURL;
            ALTER TABLE loginsM RENAME origin to hostname;
            ALTER TABLE loginsM RENAME formActionOrigin to formSubmitURL;
            INSERT INTO loginsL(guid, username, password, hostname,
            httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
            timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
            VALUES ('a', 'test', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
            'username', 'password', 1000, 1000, 1, 10, 1000, 0, 2);
            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
            usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
            is_overridden, server_modified)
            VALUES ('b', 'test', 'password', 'https://www.example.com', 'Test Realm', NULL,
            '', '', 1000, 1000, 1, 10, 0, 1000);
            PRAGMA user_version = 4;
            ",
        ).unwrap();
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
        // See todo discuss above
        //assert_eq!(row.get_raw("local_modified").as_i64().unwrap(), 1000);
        //assert_eq!(row.get_raw("is_deleted").as_i64().unwrap(), 0);
        //assert_eq!(row.get_raw("sync_status").as_i64().unwrap(), 2);

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

        // See todo discussion above
        //assert_eq!(row.get_raw("is_overridden").as_i64().unwrap(), 0);
        //assert_eq!(row.get_raw("server_modified").as_i64().unwrap(), 1000);

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
        assert_eq!(metrics.num_processed, 2);
        assert_eq!(metrics.num_succeeded, 2);
        assert_eq!(metrics.num_failed, 0);
        assert!(metrics.total_duration > 0);
        assert_eq!(metrics.errors, Vec::<String>::new());
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
            1
        );
        assert_eq!(
            db.query_one::<i32>("SELECT COUNT(*) FROM loginsM").unwrap(),
            0
        );

        // Check metrics
        assert_eq!(metrics.num_processed, 2);
        assert_eq!(metrics.num_succeeded, 1);
        assert_eq!(metrics.num_failed, 1);
        assert!(metrics.total_duration > 0);
        assert_eq!(metrics.errors.len(), 1);
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
}
