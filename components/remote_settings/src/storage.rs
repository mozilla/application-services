/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use camino::Utf8PathBuf;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json;
use std::sync::{Arc, Mutex};

use crate::{Attachment, RemoteSettingsRecord, Result};

/// Internal storage type
///
/// This will store downloaded records/attachments in a SQLite database.  Nothing is implemented
/// yet other than the initial API.
///
/// Most methods input a `base_url` parameter, which is the current base URL for the API client. If
/// the `base_url` for a get method does not match the one for a set method, then [Storage] should
/// pretend like nothing is stored in the database.
///
/// The reason for this is the [crate::RemoteSettingsService::update_config] method.  If a consumer
/// passes a new server or bucket to `update_config`, we don't want to be using cached data from
/// the previous config.
///
/// Notes:
///   - I'm thinking we'll create a separate SQLite database per collection.  That reduces
///     contention when multiple clients try to get records at once.
///   - Still, there might be contention if there are multiple clients for the same collection, or
///     if RemoteSettingsService::sync() and RemoteSettingsClient::get_records(true) are called at
///     the same time.  Maybe we should create a single write connection and put it behind a mutex
///     to avoid the possibility of SQLITE_BUSY.  Or maybe not, the writes seem like they should be
///     very fast.
///   - Maybe we should refactor this to use the DAO pattern like suggest does.
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    pub fn new(path: Utf8PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        storage.initialize_database()?;

        Ok(storage)
    }

    // Create the different tables for records and attachements for every new sqlite path
    fn initialize_database(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                base_url TEXT NOT NULL,
                data BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS attachments (
                id TEXT PRIMARY KEY,
                base_url TEXT NOT NULL,
                data BLOB NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Get the last modified timestamp for the stored records
    ///
    /// Returns None if no records are stored or if `base_url` does not match the `base_url` passed
    /// to `set_records`.
    pub fn get_last_modified_timestamp(&self, base_url: &str) -> Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT MAX(json_extract(data, '$.last_modified')) FROM records WHERE base_url = ?",
        )?;

        let result: Option<u64> = stmt.query_row(params![base_url], |row| row.get(0))?;

        Ok(result)
    }

    /// Get cached records for this collection
    ///
    /// Returns None if no records are stored or if `base_url` does not match the `base_url` passed
    /// to `set_records`.
    pub fn get_records(&self, base_url: &str) -> Result<Option<Vec<RemoteSettingsRecord>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT data FROM records WHERE base_url = ?")?;
        let mut rows = stmt.query(params![base_url])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            let data: Vec<u8> = row.get(0)?;
            let record: RemoteSettingsRecord = serde_json::from_slice(&data)?;
            records.push(record);
        }
        if records.is_empty() {
            Ok(None)
        } else {
            Ok(Some(records))
        }
    }

    pub fn get_attachment(
        &self,
        base_url: &str,
        attachment_id: &str,
    ) -> Result<Option<Attachment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT data FROM attachments WHERE id = ? AND base_url = ?")?;
        let result: Option<Vec<u8>> = stmt
            .query_row(params![attachment_id, base_url], |row| row.get(0))
            .optional()?;
        if let Some(data) = result {
            let attachment: Attachment = serde_json::from_slice(&data)?;
            Ok(Some(attachment))
        } else {
            Ok(None)
        }
    }

    /// Set the list of records stored in the database, clearing out any previously stored records
    pub fn set_records(&self, base_url: &str, records: &[RemoteSettingsRecord]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();

        let tx = conn.transaction()?;

        // Delete existing records for this base_url
        tx.execute("DELETE FROM records WHERE base_url = ?", params![base_url])?;

        // Insert new records
        {
            let mut stmt =
                tx.prepare("INSERT INTO records (id, base_url, data) VALUES (?, ?, ?)")?;
            for record in records {
                let data = serde_json::to_vec(record)?;
                stmt.execute(params![record.id, base_url, data])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn store_attachment(
        &self,
        base_url: &str,
        attachment_id: &str,
        attachment: Attachment,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let data = serde_json::to_vec(&attachment)?;
        conn.execute(
            "INSERT OR REPLACE INTO attachments (id, base_url, data) VALUES (?, ?, ?)",
            params![attachment_id, base_url, data],
        )?;
        Ok(())
    }

    /// Empty out all cached values and start from scratch.  This is called when
    /// RemoteSettingsService::update_config() is called, since that could change the remote
    /// settings server which would invalidate all cached data.
    pub fn empty(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM records", [])?;
        tx.execute("DELETE FROM attachments", [])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use camino::Utf8PathBuf;
    use super::Storage;
    use crate::{Attachment, RemoteSettingsRecord, Result};

    #[test]
    fn test_storage_set_and_get_records() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("test.db");
        let path = Utf8PathBuf::from_path_buf(db_path.clone()).unwrap();
        let storage = Storage::new(path)?;

        let base_url = "https://example.com/api";
        let records = vec![
            RemoteSettingsRecord {
                id: "1".to_string(),
                last_modified: 100,
                deleted: false,
                attachment: None,
                fields: serde_json::json!({"key": "value1"})
                    .as_object()
                    .unwrap()
                    .clone(),
            },
            RemoteSettingsRecord {
                id: "2".to_string(),
                last_modified: 200,
                deleted: false,
                attachment: None,
                fields: serde_json::json!({"key": "value2"})
                    .as_object()
                    .unwrap()
                    .clone(),
            },
        ];

        // Set records
        storage.set_records(base_url, &records)?;

        // Get records
        let fetched_records = storage.get_records(base_url)?;
        assert!(fetched_records.is_some());
        let fetched_records = fetched_records.unwrap();
        assert_eq!(fetched_records.len(), 2);
        assert_eq!(fetched_records, records);

        assert_eq!(fetched_records[0].fields["key"], "value1");

        // Get last modified timestamp
        let last_modified = storage.get_last_modified_timestamp(base_url)?;
        assert_eq!(last_modified, Some(200));

        Ok(())
    }

    #[test]
    fn test_storage_get_records_empty() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("test.db");
        let path = Utf8PathBuf::from_path_buf(db_path.clone()).unwrap();
        let storage = Storage::new(path)?;

        let base_url = "https://example.com/api";

        // Get records when none are set
        let fetched_records = storage.get_records(base_url)?;
        assert!(fetched_records.is_none());

        // Get last modified timestamp when no records
        let last_modified = storage.get_last_modified_timestamp(base_url)?;
        assert!(last_modified.is_none());

        Ok(())
    }

    #[test]
    fn test_storage_set_and_get_attachment() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("test.db");
        let path = Utf8PathBuf::from_path_buf(db_path.clone()).unwrap();
        let storage = Storage::new(path)?;

        let base_url = "https://example.com/api";
        let attachment_id = "attachment1";
        let attachment = Attachment {
            filename: "abc".to_string(),
            mimetype: "application/json".to_string(),
            location: "tmp".to_string(),
            hash: "abc123".to_string(),
            size: 1024,
        };

        // Store attachment
        storage.store_attachment(base_url, attachment_id, attachment.clone())?;

        // Get attachment
        let fetched_attachment = storage.get_attachment(base_url, attachment_id)?;
        assert!(fetched_attachment.is_some());
        let fetched_attachment = fetched_attachment.unwrap();
        assert_eq!(fetched_attachment, attachment);

        Ok(())
    }

    #[test]
    fn test_storage_get_attachment_not_found() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("test.db");
        let path = Utf8PathBuf::from_path_buf(db_path.clone()).unwrap();
        let storage = Storage::new(path)?;

        let base_url = "https://example.com/api";
        let attachment_id = "nonexistent";

        // Get attachment that doesn't exist
        let fetched_attachment = storage.get_attachment(base_url, attachment_id)?;
        assert!(fetched_attachment.is_none());

        Ok(())
    }

    #[test]
    fn test_storage_empty() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("test.db");
        let path = Utf8PathBuf::from_path_buf(db_path.clone()).unwrap();
        let storage = Storage::new(path)?;

        let base_url = "https://example.com/api";
        let records = vec![
            RemoteSettingsRecord {
                id: "1".to_string(),
                last_modified: 100,
                deleted: false,
                attachment: None,
                fields: serde_json::json!({"key": "value1"})
                    .as_object()
                    .unwrap()
                    .clone(),
            },
            RemoteSettingsRecord {
                id: "2".to_string(),
                last_modified: 200,
                deleted: false,
                attachment: None,
                fields: serde_json::json!({"key": "value2"})
                    .as_object()
                    .unwrap()
                    .clone(),
            },
        ];
        let attachment_id = "attachment1";
        let attachment = Attachment {
            filename: "abc".to_string(),
            mimetype: "application/json".to_string(),
            location: "tmp".to_string(),
            hash: "abc123".to_string(),
            size: 1024,
        };

        // Set records and attachment
        storage.set_records(base_url, &records)?;
        storage.store_attachment(base_url, attachment_id, attachment.clone())?;

        // Verify they are stored
        let fetched_records = storage.get_records(base_url)?;
        assert!(fetched_records.is_some());
        let fetched_attachment = storage.get_attachment(base_url, attachment_id)?;
        assert!(fetched_attachment.is_some());

        // Empty the storage
        storage.empty()?;

        // Verify they are deleted
        let fetched_records = storage.get_records(base_url)?;
        assert!(fetched_records.is_none());
        let fetched_attachment = storage.get_attachment(base_url, attachment_id)?;
        assert!(fetched_attachment.is_none());

        Ok(())
    }

    #[test]
    fn test_storage_base_url_isolation() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("test.db");
        let path = Utf8PathBuf::from_path_buf(db_path.clone()).unwrap();
        let storage = Storage::new(path)?;

        let base_url1 = "https://example.com/api1";
        let base_url2 = "https://example.com/api2";
        let records_base_url1 = vec![RemoteSettingsRecord {
            id: "1".to_string(),
            last_modified: 100,
            deleted: false,
            attachment: None,
            fields: serde_json::json!({"key": "value1"})
                .as_object()
                .unwrap()
                .clone(),
        }];
        let records_base_url2 = vec![RemoteSettingsRecord {
            id: "2".to_string(),
            last_modified: 200,
            deleted: false,
            attachment: None,
            fields: serde_json::json!({"key": "value2"})
                .as_object()
                .unwrap()
                .clone(),
        }];

        // Set records for base_url1
        storage.set_records(base_url1, &records_base_url1)?;
        // Set records for base_url2
        storage.set_records(base_url2, &records_base_url2)?;

        // Get records for base_url1
        let fetched_records = storage.get_records(base_url1)?;
        assert!(fetched_records.is_some());
        let fetched_records = fetched_records.unwrap();
        assert_eq!(fetched_records.len(), 1);
        assert_eq!(fetched_records, records_base_url1);

        // Get records for base_url2
        let fetched_records = storage.get_records(base_url2)?;
        assert!(fetched_records.is_some());
        let fetched_records = fetched_records.unwrap();
        assert_eq!(fetched_records.len(), 1);
        assert_eq!(fetched_records, records_base_url2);

        // Get last modified timestamps
        let last_modified1 = storage.get_last_modified_timestamp(base_url1)?;
        assert_eq!(last_modified1, Some(100));
        let last_modified2 = storage.get_last_modified_timestamp(base_url2)?;
        assert_eq!(last_modified2, Some(200));

        Ok(())
    }

    #[test]
    fn test_storage_update_records() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("test.db");
        let path = Utf8PathBuf::from_path_buf(db_path.clone()).unwrap();
        let storage = Storage::new(path)?;

        let base_url = "https://example.com/api";
        let initial_records = vec![RemoteSettingsRecord {
            id: "2".to_string(),
            last_modified: 200,
            deleted: false,
            attachment: None,
            fields: serde_json::json!({"key": "value2"})
                .as_object()
                .unwrap()
                .clone(),
        }];

        // Set initial records
        storage.set_records(base_url, &initial_records)?;

        // Verify initial records
        let fetched_records = storage.get_records(base_url)?;
        assert!(fetched_records.is_some());
        assert_eq!(fetched_records.unwrap(), initial_records);

        // Update records
        let updated_records = vec![RemoteSettingsRecord {
            id: "2".to_string(),
            last_modified: 200,
            deleted: false,
            attachment: None,
            fields: serde_json::json!({"key": "value2"})
                .as_object()
                .unwrap()
                .clone(),
        }];
        storage.set_records(base_url, &updated_records)?;

        // Verify updated records
        let fetched_records = storage.get_records(base_url)?;
        assert!(fetched_records.is_some());
        assert_eq!(fetched_records.unwrap(), updated_records);

        // Verify last modified timestamp
        let last_modified = storage.get_last_modified_timestamp(base_url)?;
        assert_eq!(last_modified, Some(200));

        Ok(())
    }
}
