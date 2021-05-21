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
    use crate::encryption::EncryptorDecryptor;
    use rusqlite::types::ValueRef;
    use std::path::PathBuf;
    use std::time;
    use sync15::ServerTimestamp;

    static TEST_SALT: &str = "01010101010101010101010101010101";
    lazy_static::lazy_static! {
        static ref TEST_KEY: String = EncryptorDecryptor::new_test_key();
        static ref TEST_ENCRYPTOR: EncryptorDecryptor = EncryptorDecryptor::new(&TEST_KEY).unwrap();
    }

    fn decrypt(value: &str) -> String {
        TEST_ENCRYPTOR.decrypt(value).unwrap()
    }
    fn encrypt(value: &str) -> String {
        TEST_ENCRYPTOR.encrypt(value).unwrap()
    }

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
        // Manually migrate back to schema v4 and insert some data
        tx.execute_batch(
            "
            ALTER TABLE loginsL RENAME usernameEnc to username;
            ALTER TABLE loginsL RENAME passwordEnc to password;
            ALTER TABLE loginsM RENAME usernameEnc to username;
            ALTER TABLE loginsM RENAME passwordEnc to password;
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
        // Run the sqlcipher DB upgrade again;
        schema::upgrade_sqlcipher_db(&mut db, &TEST_KEY).unwrap();
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
        let mut stmt = db.prepare("SELECT * FROM loginsL where guid = 'a'").unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();
        assert_eq!(decrypt(row.get_raw("usernameEnc").as_str().unwrap()), "test");
        assert_eq!(decrypt(row.get_raw("passwordEnc").as_str().unwrap()), "password");
        assert_eq!(row.get_raw("hostname").as_str().unwrap(), "https://www.example.com");
        assert_eq!(row.get_raw("httpRealm"), ValueRef::Null);
        assert_eq!(
            row.get_raw("formSubmitUrl").as_str().unwrap(),
            "https://www.example.com"
        );
        assert_eq!(row.get_raw("usernameField").as_str().unwrap(), "username");
        assert_eq!(row.get_raw("passwordField").as_str().unwrap(), "password");
        assert_eq!(row.get_raw("timeCreated").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timeLastUsed").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timePasswordChanged").as_i64().unwrap(), 1);
        assert_eq!(row.get_raw("timesUsed").as_i64().unwrap(), 10);
        assert_eq!(row.get_raw("local_modified").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("is_deleted").as_i64().unwrap(), 0);
        assert_eq!(row.get_raw("sync_status").as_i64().unwrap(), 2);

        let mut stmt = db.prepare("SELECT * FROM loginsM where guid = 'b'").unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();
        assert_eq!(decrypt(row.get_raw("usernameEnc").as_str().unwrap()), "test");
        assert_eq!(decrypt(row.get_raw("passwordEnc").as_str().unwrap()), "password");
        assert_eq!(row.get_raw("hostname").as_str().unwrap(), "https://www.example.com");
        assert_eq!(row.get_raw("httpRealm").as_str().unwrap(), "Test Realm");
        assert_eq!(row.get_raw("formSubmitUrl"), ValueRef::Null);
        assert_eq!(row.get_raw("usernameField").as_str().unwrap(), "");
        assert_eq!(row.get_raw("passwordField").as_str().unwrap(), "");
        assert_eq!(row.get_raw("timeCreated").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timeLastUsed").as_i64().unwrap(), 1000);
        assert_eq!(row.get_raw("timePasswordChanged").as_i64().unwrap(), 1);
        assert_eq!(row.get_raw("timesUsed").as_i64().unwrap(), 10);
        assert_eq!(row.get_raw("is_overridden").as_i64().unwrap(), 0);
        assert_eq!(row.get_raw("server_modified").as_i64().unwrap(), 1000);
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
