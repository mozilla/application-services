/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Code to migrate from an sqlcipher DB to a plaintext DB

use crate::error::*;
use crate::schema;
use rusqlite::{Connection, NO_PARAMS};
use sql_support::ConnExt;
use std::path::Path;

pub fn migrate_sqlcipher_db_to_plaintext(
    old_db_path: impl AsRef<Path>,
    new_db_path: impl AsRef<Path>,
    old_encryption_key: &str,
    new_encryption_key: &str,
    salt: Option<&str>,
) -> Result<()> {
    let mut db = Connection::open(old_db_path)?;
    init_sqlcipher_db(&mut db, old_encryption_key, salt, new_encryption_key)?;
    sqlcipher_export(&mut db, new_db_path)?;
    Ok(())
}

fn init_sqlcipher_db(
    db: &mut Connection,
    encryption_key: &str,
    salt: Option<&str>,
    new_encryption_key: &str,
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

    schema::upgrade_sqlcipher_db(db, new_encryption_key)?;
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

fn sqlcipher_export(conn: &mut Connection, new_db_path: impl AsRef<Path>) -> Result<()> {
    // Export sqlite data to a plaintext DB using the strategy from example 2 here:
    // https://www.zetetic.net/sqlcipher/sqlcipher-api/index.html#sqlcipher_export
    let path = new_db_path.as_ref().as_os_str();
    let path = path
        .to_str()
        .ok_or_else(|| ErrorKind::InvalidPath(path.to_os_string()))?;

    conn.execute("ATTACH DATABASE ? AS plaintext key ''", &[path])?;
    // this one is a bit weird because it's a SELECT statement that we know will return 0 rows.
    // rusqlite will return an Error if we use execute(), so we use query_row with a dummy closure
    conn.query_row("SELECT sqlcipher_export('plaintext')", NO_PARAMS, |_| Ok(0))?;
    conn.execute("DETACH DATABASE plaintext", NO_PARAMS)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LoginDb;
    use crate::login::{LocalLogin, MirrorLogin, SyncStatus};
    use std::path::PathBuf;
    use std::time;
    use sync15::ServerTimestamp;

    static TEST_SALT: &str = "01010101010101010101010101010101";

    fn create_old_db(db_path: impl AsRef<Path>, salt: Option<&str>) {
        let mut db = Connection::open(db_path).unwrap();
        db.set_pragma("key", "old-key").unwrap();
        sqlcipher_3_compat(&db).unwrap();
        if let Some(s) = salt {
            db.set_pragma("cipher_plaintext_header_size", 32).unwrap();
            db.set_pragma("cipher_salt", format!("x'{}'", s)).unwrap();
        }
        let tx = db.transaction().unwrap();
        schema::init(&tx).unwrap();
        tx.execute_batch(
            "INSERT INTO loginsL(guid, username, password, hostname,
            httpRealm, formSubmitURL, usernameField, passwordField, timeCreated, timeLastUsed,
            timePasswordChanged, timesUsed, local_modified, is_deleted, sync_status)
            VALUES ('a', 'test', 'password', 'https://www.example.com', NULL, 'https://www.example.com',
            'username', 'password', 1000, 1000, 1, 10, 1000, 0, 2);
            INSERT INTO loginsM(guid, username, password, hostname, httpRealm, formSubmitURL,
            usernameField, passwordField, timeCreated, timeLastUsed, timePasswordChanged, timesUsed,
            is_overridden, server_modified)
            VALUES ('b', 'test', 'password', 'https://www.example.com', 'Test Realm', NULL,
            '', '', 1000, 1000, 1, 10, 0, 1000);",
            ).unwrap();
        tx.commit().unwrap()
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
                old_db: tempdir.path().join(Path::new("old-db.sql")),
                new_db: tempdir.path().join(Path::new("new-db.sql")),
                _tempdir: tempdir,
            }
        }
    }

    fn check_migrated_data(db: &LoginDb) {
        let local = db.query_row_and_then(
            &format!("SELECT {}, local_modified, is_deleted, sync_status FROM loginsL WHERE guid = 'a'", &schema::COMMON_COLS),
            NO_PARAMS,
            |row| LocalLogin::from_row(row),
        ).unwrap();
        assert_eq!(local.login.username, "test");
        assert_eq!(local.login.password, "password");
        assert_eq!(local.login.hostname, "https://www.example.com");
        assert_eq!(local.login.http_realm, None);
        assert_eq!(
            local.login.form_submit_url,
            Some("https://www.example.com".to_string())
        );
        assert_eq!(local.login.username_field, "username");
        assert_eq!(local.login.password_field, "password");
        assert_eq!(local.login.time_created, 1000);
        assert_eq!(local.login.time_last_used, 1000);
        assert_eq!(local.login.time_password_changed, 1);
        assert_eq!(local.login.times_used, 10);
        assert_eq!(
            local.local_modified,
            time::UNIX_EPOCH + time::Duration::from_millis(1000)
        );
        assert_eq!(local.is_deleted, false);
        assert_eq!(local.sync_status, SyncStatus::New);

        let mirror = db
            .query_row_and_then(
                &format!(
                    "SELECT {}, is_overridden, server_modified FROM loginsM WHERE guid = 'b'",
                    &schema::COMMON_COLS
                ),
                NO_PARAMS,
                |row| MirrorLogin::from_row(row),
            )
            .unwrap();
        assert_eq!(mirror.login.username, "test");
        assert_eq!(mirror.login.password, "password");
        assert_eq!(mirror.login.hostname, "https://www.example.com");
        assert_eq!(mirror.login.http_realm, Some("Test Realm".to_string()));
        assert_eq!(mirror.login.form_submit_url, None);
        assert_eq!(mirror.login.username_field, "");
        assert_eq!(mirror.login.password_field, "");
        assert_eq!(mirror.login.time_created, 1000);
        assert_eq!(mirror.login.time_last_used, 1000);
        assert_eq!(mirror.login.time_password_changed, 1);
        assert_eq!(mirror.login.times_used, 10);
        assert_eq!(mirror.is_overridden, false);
        assert_eq!(mirror.server_modified, ServerTimestamp::from_millis(1000));
    }

    #[test]
    fn test_migrate_data() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), None);
        migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            "new-key",
            None,
        )
        .unwrap();

        // Check that the data from the old db is present in the the new DB
        let db = LoginDb::open(testpaths.new_db).unwrap();
        check_migrated_data(&db);
    }

    #[test]
    fn test_migrate_with_manual_salt() {
        let testpaths = TestPaths::new();
        create_old_db(testpaths.old_db.as_path(), Some(TEST_SALT));
        migrate_sqlcipher_db_to_plaintext(
            testpaths.old_db.as_path(),
            testpaths.new_db.as_path(),
            "old-key",
            "new-key",
            Some(TEST_SALT),
        )
        .unwrap();
        let db = LoginDb::open(testpaths.new_db).unwrap();
        check_migrated_data(&db);
    }
}
