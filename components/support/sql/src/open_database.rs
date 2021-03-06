/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Use this module to open a new SQLite database connection.
///
/// Usage:
///    - Define a struct that implements MigrationLogic.  This handles:
///      - Initializing the schema for a new database
///      - Upgrading the schema for an existing database
///      - Extra preparation/finishing steps, for example setting up SQLite functions
///
///    - Call open_database() in your database constructor:
///      - The first method called is setup_pragmas().  This is executed outside of a transaction
///        to support `PRAGMA journal_mode=wal`
///      - If the database file is not present, open_database() will create a new DB and call prepare(),
///        init(), then finish()
///      - If the database file exists, open_database() will open it and call prepare(),
///        upgrade_from() for each upgrade that needs to be applied, then finish().
///
///  See the autofill DB code for an example.
///
use crate::ConnExt;
use rusqlite::{Connection, OpenFlags, NO_PARAMS};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Incompatible database version: {0}")]
    IncompatibleVersion(u32),
    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait MigrationLogic {
    // Name to display in the logs
    const NAME: &'static str;

    // The version that the last upgrade function upgrades to.
    const END_VERSION: u32;

    // Runs first, outside of a transaction
    fn setup_pragmas(&self, _conn: &Connection) -> Result<()> {
        Ok(())
    }

    // Runs before the init/upgrade functions
    fn prepare(&self, _conn: &Connection) -> Result<()> {
        Ok(())
    }

    // Initialize a newly created database to END_VERSION
    fn init(&self, conn: &Connection) -> Result<()>;

    // Upgrade schema from version -> version + 1
    fn upgrade_from(&self, conn: &Connection, version: u32) -> Result<()>;

    // Runs after the init/upgrade functions
    fn finish(&self, _conn: &Connection) -> Result<()> {
        Ok(())
    }
}

pub fn open_database<ML: MigrationLogic, P: AsRef<Path>>(
    path: P,
    migration_logic: &ML,
) -> Result<Connection> {
    open_database_with_flags(path, OpenFlags::default(), migration_logic)
}

pub fn open_memory_database<ML: MigrationLogic>(migration_logic: &ML) -> Result<Connection> {
    open_database_with_flags(":memory:", OpenFlags::default(), migration_logic)
}

pub fn open_database_with_flags<ML: MigrationLogic, P: AsRef<Path>>(
    path: P,
    open_flags: OpenFlags,
    migration_logic: &ML,
) -> Result<Connection> {
    // Try running the migration logic with an existing file
    log::debug!("{}: opening database", ML::NAME);
    let mut conn = Connection::open_with_flags(path, open_flags)?;
    let run_init = should_init(&conn)?;

    log::debug!("{}: setup pragmas", ML::NAME);
    migration_logic.setup_pragmas(&conn)?;

    let tx = conn.transaction()?;
    log::debug!("{}: preparing database", ML::NAME);
    migration_logic.prepare(&tx)?;
    if run_init {
        log::debug!("{}: initializing new database", ML::NAME);
        migration_logic.init(&tx)?;
    } else {
        let mut current_version = get_schema_version(&tx)?;
        if current_version > ML::END_VERSION {
            return Err(Error::IncompatibleVersion(current_version));
        }
        while current_version < ML::END_VERSION {
            log::debug!(
                "{}: upgrading database to {}",
                ML::NAME,
                current_version + 1
            );
            migration_logic.upgrade_from(&tx, current_version)?;
            current_version += 1;
        }
    }
    log::debug!("{}: finishing database open", ML::NAME);
    migration_logic.finish(&tx)?;
    set_schema_version(&tx, ML::END_VERSION)?;
    tx.commit()?;
    log::debug!("{}: database open successful", ML::NAME);

    Ok(conn)
}

fn should_init(conn: &Connection) -> Result<bool> {
    Ok(conn.query_one::<u32>("SELECT COUNT(*) FROM sqlite_master")? == 0)
}

fn get_schema_version(conn: &Connection) -> Result<u32> {
    let version = conn.query_row_and_then("PRAGMA user_version", NO_PARAMS, |row| row.get(0))?;
    Ok(version)
}

fn set_schema_version(conn: &Connection, version: u32) -> Result<()> {
    conn.set_pragma("user_version", version)?;
    Ok(())
}

// It would be nice for this to be #[cfg(test)], but that doesn't allow it to be used in tests for
// our other crates.
pub mod test_utils {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Database file that we can programatically run upgrades on
    //
    // We purposefully don't keep a connection to the database around to force upgrades to always
    // run against a newly opened DB, like they would in the real world.  See #4106 for
    // details.
    pub struct MigratedDatabaseFile<ML: MigrationLogic> {
        // Keep around a TempDir to ensure the database file stays around until this struct is
        // dropped
        _tempdir: TempDir,
        pub migration_logic: ML,
        pub path: PathBuf,
    }

    impl<ML: MigrationLogic> MigratedDatabaseFile<ML> {
        pub fn new(migration_logic: ML, init_sql: &str) -> Self {
            Self::new_with_flags(migration_logic, init_sql, OpenFlags::default())
        }

        pub fn new_with_flags(migration_logic: ML, init_sql: &str, open_flags: OpenFlags) -> Self {
            let tempdir = tempfile::tempdir().unwrap();
            let path = tempdir.path().join(Path::new("db.sql"));
            let conn = Connection::open_with_flags(&path, open_flags).unwrap();
            conn.execute_batch(init_sql).unwrap();
            Self {
                _tempdir: tempdir,
                migration_logic,
                path,
            }
        }

        pub fn upgrade_to(&self, version: u32) {
            let conn = self.open();
            self.migration_logic.prepare(&conn).unwrap();
            let mut current_version = get_schema_version(&conn).unwrap();
            while current_version < version {
                self.migration_logic
                    .upgrade_from(&conn, current_version)
                    .unwrap();
                current_version += 1;
            }
            set_schema_version(&conn, current_version).unwrap();
            self.migration_logic.finish(&conn).unwrap();
        }

        pub fn run_all_upgrades(&self) {
            let current_version = get_schema_version(&self.open()).unwrap();
            for version in current_version..ML::END_VERSION {
                self.upgrade_to(version + 1);
            }
        }

        pub fn open(&self) -> Connection {
            Connection::open(&self.path).unwrap()
        }
    }
}

// Disabled because it breaks `cargo test` from the root director because of NSS linking issues.
// It seems that any test code that calls one of the `rusqlite::Connection::open*` calls causes
// this.
//
// #[cfg(test)]
// mod test {
//     use super::test_utils::MigratedDatabaseFile;
//     use super::*;
//     use std::cell::RefCell;
//
//     struct TestMigrationLogic {
//         pub calls: RefCell<Vec<&'static str>>,
//         pub buggy_v3_upgrade: bool,
//     }
//
//     impl TestMigrationLogic {
//         pub fn new() -> Self {
//             Self {
//                 calls: RefCell::new(Vec::new()),
//                 buggy_v3_upgrade: false,
//             }
//         }
//         pub fn new_with_buggy_logic() -> Self {
//             Self {
//                 calls: RefCell::new(Vec::new()),
//                 buggy_v3_upgrade: true,
//             }
//         }
//
//         pub fn clear_calls(&self) {
//             self.calls.borrow_mut().clear();
//         }
//
//         pub fn push_call(&self, call: &'static str) {
//             self.calls.borrow_mut().push(call);
//         }
//
//         pub fn check_calls(&self, expected: Vec<&'static str>) {
//             assert_eq!(*self.calls.borrow(), expected);
//         }
//     }
//
//     impl MigrationLogic for TestMigrationLogic {
//         const NAME: &'static str = "test db";
//         const END_VERSION: u32 = 4;
//
//         fn setup_pragmas(&self, conn: &Connection) -> Result<()> {
//             conn.execute_batch("PRAGMA journal_mode = wal;")?;
//             Ok(())
//         }
//
//         fn prepare(&self, conn: &Connection) -> Result<()> {
//             self.push_call("prep");
//             conn.execute_batch(
//                 "
//                 CREATE TABLE prep_table(col);
//                 INSERT INTO prep_table(col) VALUES ('correct-value');
//                 ",
//             )?;
//             Ok(())
//         }
//
//         fn init(&self, conn: &Connection) -> Result<()> {
//             self.push_call("init");
//             conn.execute_batch(
//                 "
//                 CREATE TABLE my_table(col);
//                 ",
//             )
//             .map_err(|e| e.into())
//         }
//
//         fn upgrade_from(&self, conn: &Connection, version: u32) -> Result<()> {
//             match version {
//                 2 => {
//                     self.push_call("upgrade_from_v2");
//                     conn.execute_batch(
//                         "
//                         ALTER TABLE my_old_table_name RENAME TO my_table;
//                         ",
//                     )?;
//                     Ok(())
//                 }
//                 3 => {
//                     self.push_call("upgrade_from_v3");
//
//                     if self.buggy_v3_upgrade {
//                         conn.execute_batch("ILLEGAL_SQL_CODE")?;
//                     }
//
//                     conn.execute_batch(
//                         "
//                         ALTER TABLE my_table RENAME COLUMN old_col to col;
//                         ",
//                     )?;
//                     Ok(())
//                 }
//                 _ => {
//                     panic!("Unexpected version: {}", version);
//                 }
//             }
//         }
//
//         fn finish(&self, conn: &Connection) -> Result<()> {
//             self.push_call("finish");
//             conn.execute_batch(
//                 "
//                 INSERT INTO my_table(col) SELECT col FROM prep_table;
//                 DROP TABLE prep_table;
//                 ",
//             )?;
//             Ok(())
//         }
//     }
//
//     // Initialize the database to v2 to test upgrading from there
//     static INIT_V2: &str = "
//         CREATE TABLE my_old_table_name(old_col);
//         PRAGMA user_version=2;
//     ";
//
//     fn check_final_data(conn: &Connection) {
//         let value: String = conn
//             .query_row("SELECT col FROM my_table", NO_PARAMS, |r| r.get(0))
//             .unwrap();
//         assert_eq!(value, "correct-value");
//         assert_eq!(get_schema_version(&conn).unwrap(), 4);
//     }
//
//     #[test]
//     fn test_init() {
//         let migration_logic = TestMigrationLogic::new();
//         let conn = open_memory_database(&migration_logic).unwrap();
//         check_final_data(&conn);
//         migration_logic.check_calls(vec!["prep", "init", "finish"]);
//     }
//
//     #[test]
//     fn test_upgrades() {
//         let db_file = MigratedDatabaseFile::new(TestMigrationLogic::new(), INIT_V2);
//         let conn = open_database(db_file.path.clone(), &db_file.migration_logic).unwrap();
//         check_final_data(&conn);
//         db_file.migration_logic.check_calls(vec![
//             "prep",
//             "upgrade_from_v2",
//             "upgrade_from_v3",
//             "finish",
//         ]);
//     }
//
//     #[test]
//     fn test_open_current_version() {
//         let db_file = MigratedDatabaseFile::new(TestMigrationLogic::new(), INIT_V2);
//         db_file.upgrade_to(4);
//         db_file.migration_logic.clear_calls();
//         let conn = open_database(db_file.path.clone(), &db_file.migration_logic).unwrap();
//         check_final_data(&conn);
//         db_file.migration_logic.check_calls(vec!["prep", "finish"]);
//     }
//
//     #[test]
//     fn test_pragmas() {
//         let db_file = MigratedDatabaseFile::new(TestMigrationLogic::new(), INIT_V2);
//         let conn = open_database(db_file.path.clone(), &db_file.migration_logic).unwrap();
//         assert_eq!(
//             conn.query_one::<String>("PRAGMA journal_mode").unwrap(),
//             "wal"
//         );
//     }
//
//     #[test]
//     fn test_migration_error() {
//         let db_file =
//             MigratedDatabaseFile::new(TestMigrationLogic::new_with_buggy_logic(), INIT_V2);
//         db_file
//             .open()
//             .execute(
//                 "INSERT INTO my_old_table_name(old_col) VALUES ('I should not be deleted')",
//                 NO_PARAMS,
//             )
//             .unwrap();
//
//         open_database(db_file.path.clone(), &db_file.migration_logic).unwrap_err();
//         // Even though the upgrades failed, the data should still be there.  The changes that
//         // upgrade_to_v3 made should have been rolled back.
//         assert_eq!(
//             db_file
//                 .open()
//                 .query_one::<i32>("SELECT COUNT(*) FROM my_old_table_name")
//                 .unwrap(),
//             1
//         );
//     }
//
//     #[test]
//     fn test_version_too_new() {
//         let db_file = MigratedDatabaseFile::new(TestMigrationLogic::new(), INIT_V2);
//         set_schema_version(&db_file.open(), 5).unwrap();
//
//         db_file
//             .open()
//             .execute(
//                 "INSERT INTO my_old_table_name(old_col) VALUES ('I should not be deleted')",
//                 NO_PARAMS,
//             )
//             .unwrap();
//
//         assert!(matches!(
//             open_database(db_file.path.clone(), &db_file.migration_logic,),
//             Err(Error::IncompatibleVersion(5))
//         ));
//         // Make sure that even when DeleteAndRecreate is specified, we don't delete the database
//         // file when the schema is newer
//         assert_eq!(
//             db_file
//                 .open()
//                 .query_one::<i32>("SELECT COUNT(*) FROM my_old_table_name")
//                 .unwrap(),
//             1
//         );
//     }
// }
