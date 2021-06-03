/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Use this module to open a new SQLite database connection.
///
/// The code handles some common cases:
///
///   - Opening new databases.  If this is the first time opening the database, then initialize it
///     to the current schema.
///
///   - Migrating existing databases.  If this is an existing database, then run a series of
///     upgrade functions to migrate it to the current schema.
///
///   - Handling migration failures.   If opening or migrating a database results in an error,
///     we can optionally delete the database file and create a new one.
use crate::ConnExt;
use rusqlite::{Connection, OpenFlags, NO_PARAMS};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // Generic error meaning that something went wrong during the migration.  You can return this
    // from your upgrade functions to signal that the database is beyond repair and can't be
    // migrated.
    #[error("MigrationError: {0}")]
    MigrationError(String),
    // Error with the migration logic struct, for example the number upgrade functions doesn't
    // line up with start_version and end_version
    #[error("MigrationLogicError: {0}")]
    MigrationLogicError(String),
    #[error("Database version too old: {0}")]
    VersionTooOld(u32),
    #[error("Database version too new: {0}")]
    VersionTooNew(u32),
    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),
    #[error("IOError")]
    IOError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
pub type DatabaseFunc = fn(&Connection) -> Result<()>;

pub enum DatabaseLocation {
    File(PathBuf),
    Memory,
}

impl DatabaseLocation {
    fn exists(&self) -> bool {
        match self {
            DatabaseLocation::Memory => false,
            DatabaseLocation::File(path) => path.exists(),
        }
    }

    fn open(&self, open_flags: OpenFlags) -> Result<Connection> {
        match self {
            DatabaseLocation::Memory => Ok(Connection::open_in_memory_with_flags(open_flags)?),
            DatabaseLocation::File(path) => Ok(Connection::open_with_flags(path, open_flags)?),
        }
    }

    fn delete(&self) -> Result<()> {
        match self {
            DatabaseLocation::Memory => (),
            DatabaseLocation::File(path) => {
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq)]
pub enum ErrorHandling {
    // Try to start over with a fresh database.  This is the behavior that most components should
    // use (See SYNC-937, SYNC-816)
    DeleteAndRecreate,
    // Return an error from open function.  This allows us to potentially recover the data with an
    // upgraded version
    ReturnError,
}

#[derive(Clone)]
pub struct MigrationLogic {
    // Name to display in the logs
    pub name: String,
    // The first version that this migration applies to (usually 1)
    pub start_version: u32,
    // The version that the last upgrade function upgrades to.
    // Note: this is intentionally redundant so that it can act as a sanity check on the length of
    // the upgrades vec.
    pub end_version: u32,
    // Runs before the init/upgrade functions
    pub prepare: Option<DatabaseFunc>,
    // Initialize a newly created database to <end_version>
    pub init: DatabaseFunc,
    // Upgrade functions.  upgrades[n] will migrate version n to n+1
    pub upgrades: Vec<DatabaseFunc>,
    // Runs after the init/upgrade functions
    pub finish: Option<DatabaseFunc>,
    // How to handle migration errors
    pub error_handling: ErrorHandling,
}

impl MigrationLogic {
    fn sanity_check(&self) -> Result<()> {
        let total_versions = (self.end_version - self.start_version) as usize;
        match self.upgrades.len() {
            x if x < total_versions => Err(Error::MigrationLogicError(format!(
                "Not enough upgrade functions to upgrade from {} to {}",
                self.start_version, self.end_version
            ))),
            x if x > total_versions => Err(Error::MigrationLogicError(format!(
                "Too many upgrade functions to upgrade from {} to {}",
                self.start_version, self.end_version
            ))),
            _ => Ok(()),
        }
    }

    fn run(&self, conn: &Connection, init: bool) -> Result<()> {
        log::debug!("{}: opening database", self.name);
        let tx = conn.unchecked_transaction()?;
        self.run_prepare(&tx)?;
        if init {
            self.run_init(&tx)?;
        } else {
            let mut current_version = get_schema_version(&tx)?;
            if current_version < self.start_version {
                return Err(Error::VersionTooOld(current_version));
            } else if current_version > self.end_version {
                return Err(Error::VersionTooNew(current_version));
            }
            while current_version < self.end_version {
                self.run_upgrade(&tx, current_version + 1)?;
                current_version += 1;
            }
        }
        set_schema_version(&tx, self.end_version)?;
        self.run_finish(&tx)?;
        tx.commit()?;
        log::debug!("{}: database open successful", self.name);
        Ok(())
    }

    fn run_prepare(&self, conn: &Connection) -> Result<()> {
        log::debug!("{}: preparing database", self.name);
        if let Some(prepare) = self.prepare {
            prepare(&conn)?;
        }
        Ok(())
    }

    fn run_init(&self, conn: &Connection) -> Result<()> {
        log::debug!("{}: initializing new database", self.name);
        (self.init)(&conn)?;
        Ok(())
    }

    // Run the upgrade function to upgrade to v[version]
    // This will panic unless start_version < version <= end_version.
    fn run_upgrade(&self, conn: &Connection, version: u32) -> Result<()> {
        log::debug!("{}: upgrading database to {}", self.name, version);
        let upgrade_index = (version - self.start_version - 1) as usize;
        (self.upgrades[upgrade_index])(&conn)?;
        Ok(())
    }

    fn run_finish(&self, conn: &Connection) -> Result<()> {
        log::debug!("{}: finishing database open", self.name);
        if let Some(finish) = self.finish {
            finish(&conn)?;
        }
        Ok(())
    }
}

pub fn open_database(path: PathBuf, migration_logic: MigrationLogic) -> Result<Connection> {
    open_database_with_flags(
        DatabaseLocation::File(path),
        OpenFlags::default(),
        migration_logic,
    )
}

pub fn open_database_with_flags(
    location: DatabaseLocation,
    open_flags: OpenFlags,
    migration_logic: MigrationLogic,
) -> Result<Connection> {
    migration_logic.sanity_check()?;
    // Try running the migration logic with an existing file
    let initializing = !location.exists();
    let mut conn = location.open(open_flags)?;
    let mut result = migration_logic.run(&conn, initializing);
    // If that failed, maybe try again with a fresh database
    if migration_logic.error_handling == ErrorHandling::DeleteAndRecreate {
        result = result.or_else(|e| {
            log::warn!(
                "Error while opening database file, will recreate file: {:?}",
                e
            );
            location.delete()?;
            conn = location.open(open_flags)?;
            migration_logic.run(&conn, true)
        })
    }
    result?;

    Ok(conn)
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
    use std::path::Path;
    use tempfile::TempDir;

    pub fn open_memory_database(migration: MigrationLogic) -> Result<Connection> {
        open_database_with_flags(DatabaseLocation::Memory, OpenFlags::default(), migration)
    }

    // Database file that we can programatically run upgrades on
    //
    // We purposefully don't keep a connection to the database around to force upgrades to always
    // run against a newly opened DB, like they would in the real world.  See SYNC-2209 for
    // details.
    pub struct MigratedDatabaseFile {
        // Keep around a TempDir to ensure the database file stays around until this struct is
        // dropped
        _tempdir: TempDir,
        migration_logic: MigrationLogic,
        pub path: PathBuf,
    }

    impl MigratedDatabaseFile {
        pub fn new(
            migration_logic: MigrationLogic,
            initial_schema_func: DatabaseFunc,
            initial_version: u32,
        ) -> Self {
            Self::new_with_flags(
                migration_logic,
                initial_schema_func,
                initial_version,
                OpenFlags::default(),
            )
        }

        pub fn new_with_flags(
            migration_logic: MigrationLogic,
            initial_schema_func: DatabaseFunc,
            initial_version: u32,
            open_flags: OpenFlags,
        ) -> Self {
            let tempdir = tempfile::tempdir().unwrap();
            let path = tempdir.path().join(Path::new("db.sql"));
            let conn = Connection::open_with_flags(&path, open_flags).unwrap();
            initial_schema_func(&conn).unwrap();
            set_schema_version(&conn, initial_version).unwrap();
            Self {
                _tempdir: tempdir,
                migration_logic,
                path,
            }
        }

        pub fn upgrade_to(&self, version: u32) {
            // Create a migration logic with a subset of our upgrades
            let upgrade_count = (version - self.migration_logic.start_version) as usize;
            let upgrades = (&self.migration_logic.upgrades[..upgrade_count]).to_vec();
            let upgrade_logic = MigrationLogic {
                end_version: version,
                upgrades,
                ..self.migration_logic.clone()
            };
            upgrade_logic.run(&self.open(), false).unwrap();
        }

        pub fn run_all_upgrades(&self) {
            for version in self.migration_logic.start_version..self.migration_logic.end_version {
                self.upgrade_to(version + 1);
            }
        }

        pub fn open(&self) -> Connection {
            Connection::open(&self.path).unwrap()
        }
    }
}

#[cfg(test)]
mod test {
    use super::test_utils::MigratedDatabaseFile;
    use super::*;

    // Use a DB table to check which functions were called and in what order
    fn init_call_table(conn: &Connection) {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS call_table(name)")
            .unwrap();
    }
    fn clear_calls(conn: &Connection) {
        conn.execute_batch("DELETE FROM call_table").unwrap()
    }
    fn push_call(conn: &Connection, name: &'static str) {
        conn.execute("INSERT INTO call_table(name) VALUES (?)", &[name])
            .unwrap();
    }
    fn get_calls(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("SELECT name FROM call_table").unwrap();
        stmt.query_map(NO_PARAMS, |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    // Migration code that can upgrade from v2 to v4
    fn prep(conn: &Connection) -> Result<()> {
        init_call_table(&conn);
        push_call(&conn, "prep");
        conn.execute_batch(
            "
            CREATE TABLE prep_table(col);
            INSERT INTO prep_table(col) VALUES ('correct-value');
            ",
        )?;
        Ok(())
    }

    fn init(conn: &Connection) -> Result<()> {
        push_call(&conn, "init");
        conn.execute_batch(
            "
            CREATE TABLE my_table(col);
            ",
        )
        .map_err(|e| e.into())
    }

    fn upgrade_to_v3(conn: &Connection) -> Result<()> {
        push_call(&conn, "upgrade_to_v3");
        conn.execute_batch(
            "
            ALTER TABLE my_old_table_name RENAME TO my_table;
            ",
        )
        .map_err(|e| e.into())
    }

    fn upgrade_to_v4(conn: &Connection) -> Result<()> {
        push_call(&conn, "upgrade_to_v4");
        conn.execute_batch(
            "
            ALTER TABLE my_table RENAME COLUMN old_col to col;
            ",
        )
        .map_err(|e| e.into())
    }

    fn buggy_upgrade_to_v4(_conn: &Connection) -> Result<()> {
        Err(Error::MigrationError("Test error".to_string()))
    }

    fn finish(conn: &Connection) -> Result<()> {
        push_call(&conn, "finish");
        conn.execute_batch(
            "
            INSERT INTO my_table(col) SELECT col FROM prep_table;
            DROP TABLE prep_table;
            ",
        )?;
        Ok(())
    }

    // Initialize the database to v2 to test upgrading from there
    fn init_v2(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE my_old_table_name(old_col);
            ",
        )
        .map_err(|e| e.into())
    }

    fn test_migration_logic() -> MigrationLogic {
        MigrationLogic {
            name: "test db".to_string(),
            start_version: 2,
            end_version: 4,
            prepare: Some(prep),
            init,
            upgrades: vec![upgrade_to_v3, upgrade_to_v4],
            finish: Some(finish),
            error_handling: ErrorHandling::ReturnError,
        }
    }

    fn check_final_data(conn: &Connection) {
        let value: String = conn
            .query_row("SELECT col FROM my_table", NO_PARAMS, |r| r.get(0))
            .unwrap();
        assert_eq!(value, "correct-value");
        assert_eq!(get_schema_version(&conn).unwrap(), 4);
    }

    #[test]
    fn test_init() {
        let conn = test_utils::open_memory_database(test_migration_logic()).unwrap();
        check_final_data(&conn);
        assert_eq!(get_calls(&conn), vec!["prep", "init", "finish"]);
    }

    #[test]
    fn test_upgrades() {
        let db_file = MigratedDatabaseFile::new(test_migration_logic(), init_v2, 2);
        let conn = open_database(db_file.path, test_migration_logic()).unwrap();
        check_final_data(&conn);
        assert_eq!(
            get_calls(&conn),
            vec!["prep", "upgrade_to_v3", "upgrade_to_v4", "finish"]
        );
    }

    #[test]
    fn test_open_current_version() {
        let db_file = MigratedDatabaseFile::new(test_migration_logic(), init_v2, 2);
        db_file.upgrade_to(4);
        clear_calls(&db_file.open());
        let conn = open_database(db_file.path, test_migration_logic()).unwrap();
        check_final_data(&conn);
        assert_eq!(get_calls(&conn), vec!["prep", "finish"]);
    }

    #[test]
    fn test_error_handling_delete_and_recreate() {
        // Create a migration logic where the upgrade will fail, then we will recreate the DB file
        let migration_logic = MigrationLogic {
            upgrades: vec![upgrade_to_v3, buggy_upgrade_to_v4],
            error_handling: ErrorHandling::DeleteAndRecreate,
            ..test_migration_logic()
        };
        let db_file = MigratedDatabaseFile::new(migration_logic.clone(), init_v2, 2);
        // Insert some data into the database, this should be deleted when we recreate the file
        db_file
            .open()
            .execute(
                "INSERT INTO my_old_table_name(old_col) VALUES ('I should be deleted')",
                NO_PARAMS,
            )
            .unwrap();

        let conn = open_database(db_file.path, migration_logic).unwrap();
        check_final_data(&conn);
    }

    #[test]
    fn test_error_handling_return_error() {
        // Create a migration logic where the upgrade will fail and we should return the failure
        let migration_logic = MigrationLogic {
            upgrades: vec![upgrade_to_v3, buggy_upgrade_to_v4],
            error_handling: ErrorHandling::ReturnError,
            ..test_migration_logic()
        };
        let db_file = MigratedDatabaseFile::new(migration_logic.clone(), init_v2, 2);
        db_file
            .open()
            .execute(
                "INSERT INTO my_old_table_name(old_col) VALUES ('I should not be deleted')",
                NO_PARAMS,
            )
            .unwrap();

        assert!(matches!(
            open_database(db_file.path.clone(), migration_logic),
            Err(Error::MigrationError(_))
        ));
        // Even though the upgrades failed, the data should still be there.  The changes that
        // upgrade_to_v3 made should have been rolled back.
        assert_eq!(
            db_file
                .open()
                .query_one::<i32>("SELECT COUNT(*) FROM my_old_table_name")
                .unwrap(),
            1
        );
    }

    #[test]
    fn test_version_too_new() {
        let db_file = MigratedDatabaseFile::new(test_migration_logic(), init_v2, 5);
        assert!(matches!(
            open_database(db_file.path, test_migration_logic()),
            Err(Error::VersionTooNew(5))
        ));
    }

    #[test]
    fn test_version_too_old() {
        let db_file = MigratedDatabaseFile::new(test_migration_logic(), init_v2, 1);
        assert!(matches!(
            open_database(db_file.path, test_migration_logic()),
            Err(Error::VersionTooOld(1))
        ));
    }

    #[test]
    fn test_upgrade_functions_dont_match_versions() {
        let too_few_upgrade_funcs = MigrationLogic {
            upgrades: vec![upgrade_to_v3],
            ..test_migration_logic()
        };

        let too_many_upgrade_funcs = MigrationLogic {
            upgrades: vec![upgrade_to_v3, upgrade_to_v4, upgrade_to_v4],
            ..test_migration_logic()
        };

        assert!(matches!(
            test_utils::open_memory_database(too_few_upgrade_funcs),
            Err(Error::MigrationLogicError(_))
        ));
        assert!(matches!(
            test_utils::open_memory_database(too_many_upgrade_funcs),
            Err(Error::MigrationLogicError(_))
        ));
    }
}
