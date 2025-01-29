/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

//! Database initialization, schema and migration code
//!
//! The `sql_support` crate provides decent support for SQLite migrations.
//! The basic system is:
//!   * Define the current schema at the top of the module, alongside a version number
//!   * If you change the schema then:
//!      * Update the current schema
//!      * Bump the version number
//!      * Add the migration as a case in the `upgrade_from` function

use rusqlite::{Connection, Transaction};
use sql_support::{
    open_database::{self, ConnectionInitializer},
    setup_sqlite_defaults,
};

/// Current schema version.
pub const VERSION: u32 = 2;

/// Current database schema.
pub const SQL: &str = r#"
CREATE TABLE todo_list(
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE TABLE todo(
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    list_id INTEGER,
    completed INTEGER,
    last_modified INTEGER,
    url TEXT,
    FOREIGN KEY(list_id) REFERENCES todo_list(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX todo_list_name ON todo_list(name);
CREATE UNIQUE INDEX todo_list_id_name ON todo(list_id, name);
-- Note: Create an index for every foreign key column, unless you have a good reason not to.
-- Otherwise looking up the children of a parent requires a full table scan. Even if you don't do
-- that in your component, it's what SQLite will do when deleting a parent row and `PRAGMA
-- foreign_keys = ON` is set
CREATE INDEX todo_list_id ON todo(list_id);
"#;

/// Placeholder type that we implement the database initialization trait on.
#[derive(Default)]
pub struct ExampleComponentConnectionInitializer;

impl ConnectionInitializer for ExampleComponentConnectionInitializer {
    const NAME: &'static str = "example-component";
    const END_VERSION: u32 = VERSION;

    /// Setup PRAGMAs for a new SQLite connection
    ///
    /// In general, just use `setup_sqlite_defaults` and maybe turn on foreign keys. It's
    /// recommended to start with foreign key support on, but consider turning them off if you
    /// notice performance issues with your component.
    ///
    /// If you have questions, ask the `#app-storage` channel in Slack.
    fn prepare(&self, conn: &Connection, _db_empty: bool) -> open_database::Result<()> {
        setup_sqlite_defaults(conn)?;
        conn.execute("PRAGMA foreign_keys = ON", ())?;

        Ok(())
    }

    /// Initialize the database, which just means executing the SQL from the top of the module
    fn init(&self, db: &Transaction<'_>) -> open_database::Result<()> {
        db.execute_batch(SQL)?;
        Ok(())
    }

    /// Upgrade the database.  This inputs the version number that we're upgrading from and should
    /// run whatever SQL is needed to upgrade
    fn upgrade_from(&self, tx: &Transaction<'_>, version: u32) -> open_database::Result<()> {
        match version {
            1 => {
                // Version 2 added the `url` field
                tx.execute_batch("ALTER TABLE todo ADD COLUMN url TEXT")?;
                Ok(())
            }
            _ => Err(open_database::Error::IncompatibleVersion(version)),
        }
    }
}

// The `sql_support` crate has support for testing that upgraded schemas match a newly created schema.
#[cfg(test)]
mod test {
    use super::*;
    use sql_support::open_database::test_utils::MigratedDatabaseFile;

    // Snapshot of the v1 schema.  We use this to test that we can migrate from there to the
    // current schema.  Make sure to include the `PRAGMA user_version` at the bottom so that the
    // upgrade code runs
    const V1_SCHEMA: &str = r#"
CREATE TABLE todo_list(
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE TABLE todo(
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    list_id INTEGER,
    completed INTEGER,
    last_modified INTEGER,
    FOREIGN KEY(list_id) REFERENCES todo_list(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX todo_list_name ON todo_list(name);
CREATE UNIQUE INDEX todo_list_id_name ON todo(list_id, name);
CREATE INDEX todo_list_id ON todo(list_id);
PRAGMA user_version=1;
"#;

    /// Standard schema upgrade test.  Copy and paste this into your component.
    #[test]
    fn test_all_upgrades() {
        let db_file = MigratedDatabaseFile::new(ExampleComponentConnectionInitializer, V1_SCHEMA);
        db_file.run_all_upgrades();
        db_file.assert_schema_matches_new_database();
    }
}
