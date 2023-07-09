use rusqlite::{Connection, Transaction};
use sql_support::open_database::{ConnectionInitializer, Error, Result};

pub const VERSION: u32 = 1;

pub const SQL: &str = "
    CREATE TABLE meta(
        key TEXT PRIMARY KEY,
        value NOT NULL
    ) WITHOUT ROWID;

    CREATE TABLE keywords(
        keyword TEXT NOT NULL,
        suggestion_id INTEGER NOT NULL REFERENCES suggestions(id) ON DELETE CASCADE,
        rank INTEGER NOT NULL,
        PRIMARY KEY (keyword, suggestion_id)
    ) WITHOUT ROWID;

    CREATE UNIQUE INDEX keywords_suggestion_id_rank ON keywords(suggestion_id, rank);

    CREATE TABLE suggestions(
        id INTEGER PRIMARY KEY,
        record_id TEXT NOT NULL,
        block_id INTEGER NOT NULL,
        advertiser TEXT NOT NULL,
        iab_category TEXT NOT NULL,
        title TEXT NOT NULL,
        url TEXT NOT NULL,
        icon_id TEXT NOT NULL,
        impression_url TEXT,
        click_url TEXT
    );

    CREATE INDEX suggestions_record_id ON suggestions(record_id);

    CREATE TABLE icons(
        id TEXT PRIMARY KEY,
        data BLOB NOT NULL
    ) WITHOUT ROWID;
";

pub struct SuggestConnectionInitializer;

impl ConnectionInitializer for SuggestConnectionInitializer {
    const NAME: &'static str = "suggest db";
    const END_VERSION: u32 = VERSION;

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> Result<()> {
        let initial_pragmas = "
            -- Use in-memory storage for TEMP tables.
            PRAGMA temp_store = 2;

            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
        ";
        conn.execute_batch(initial_pragmas)?;

        Ok(())
    }

    fn init(&self, db: &Transaction<'_>) -> Result<()> {
        Ok(db.execute_batch(SQL)?)
    }

    fn upgrade_from(&self, _db: &Transaction<'_>, version: u32) -> Result<()> {
        Err(Error::IncompatibleVersion(version))
    }
}
