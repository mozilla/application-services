use crate::error::*;
use crate::storage::{db::RemergeDb, NativeRecord, NativeSchemaAndText};
use crate::Guid;
use std::convert::TryFrom;
use std::path::Path;
/// "Friendly" public api for using Remerge.
pub struct RemergeEngine {
    pub db: RemergeDb,
}

impl RemergeEngine {
    pub fn open(path: impl AsRef<Path>, schema_json: &str) -> Result<Self> {
        let schema = NativeSchemaAndText::try_from(schema_json)?;
        let conn = rusqlite::Connection::open(path.as_ref())?;
        let db = RemergeDb::with_connection(conn, schema)?;
        Ok(Self { db })
    }

    pub fn open_in_memory(schema_json: &str) -> Result<Self> {
        let schema = NativeSchemaAndText::try_from(schema_json)?;
        let conn = rusqlite::Connection::open_in_memory()?;
        let db = RemergeDb::with_connection(conn, schema)?;
        Ok(Self { db })
    }

    pub fn list(&self) -> Result<Vec<NativeRecord>> {
        self.db.get_all()
    }

    pub fn get(&self, id: &str) -> Result<Option<NativeRecord>> {
        self.db.get_by_id(id)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.delete_by_id(id)
    }

    pub fn update(&self, rec: &NativeRecord) -> Result<()> {
        self.db.update_record(rec)
    }

    pub fn add(&self, rec: &NativeRecord) -> Result<Guid> {
        self.db.create(rec)
    }
}
