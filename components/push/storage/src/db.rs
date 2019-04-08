use std::{ops::Deref, path::Path};

use rusqlite::Connection;
use sql_support::ConnExt;

use push_errors::{ErrorKind, Result};

use crate::{record::PushRecord, schema};

// TODO: Add broadcasts storage

pub trait Storage {
    fn get_record(&self, uaid: &str, chid: &str) -> Result<Option<PushRecord>>;

    fn get_record_by_chid(&self, chid: &str) -> Result<Option<PushRecord>>;

    fn put_record(&self, record: &PushRecord) -> Result<bool>;

    fn delete_record(&self, uaid: &str, chid: &str) -> Result<bool>;

    fn delete_all_records(&self, uaid: &str) -> Result<()>;

    fn get_channel_list(&self, uaid: &str) -> Result<Vec<String>>;

    fn update_endpoint(&self, uaid: &str, channel_id: &str, endpoint: &str) -> Result<bool>;

    fn update_native_id(&self, uaid: &str, native_id: &str) -> Result<bool>;

    fn get_meta(&self, key: &str) -> Result<Option<String>>;

    fn set_meta(&self, key: &str, value: &str) -> Result<()>;
}

pub struct PushDb {
    pub db: Connection,
}

impl PushDb {
    pub fn with_connection(db: Connection) -> Result<Self> {
        // XXX: consider the init_test_logging call in other components
        schema::init(&db)?;
        Ok(Self { db })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        // By default, file open errors are StorageSqlErrors and aren't super helpful.
        // Instead, remap to StorageError and provide the path to the file that couldn't be opened.
        Ok(Self::with_connection(Connection::open(&path).map_err(
            |_| {
                ErrorKind::StorageError(format!(
                    "Could not open database file {:?}",
                    &path.as_ref().as_os_str()
                ))
            },
        )?)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self::with_connection(conn)?)
    }
}

impl Deref for PushDb {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.db
    }
}

impl ConnExt for PushDb {
    fn conn(&self) -> &Connection {
        &self.db
    }
}

impl Storage for PushDb {
    fn get_record(&self, uaid: &str, chid: &str) -> Result<Option<PushRecord>> {
        let query = format!(
            "SELECT {common_cols}
             FROM push_record WHERE uaid = :uaid AND channel_id = :chid",
            common_cols = schema::COMMON_COLS,
        );
        Ok(self.try_query_row(
            &query,
            &[(":uaid", &uaid), (":chid", &chid)],
            PushRecord::from_row,
            false,
        )?)
    }

    fn get_record_by_chid(&self, chid: &str) -> Result<Option<PushRecord>> {
        let query = format!(
            "SELECT {common_cols}
             FROM push_record WHERE channel_id = :chid",
            common_cols = schema::COMMON_COLS,
        );
        Ok(self.try_query_row(&query, &[(":chid", &chid)], PushRecord::from_row, false)?)
    }

    fn put_record(&self, record: &PushRecord) -> Result<bool> {
        let query = format!(
            "INSERT INTO push_record
                 ({common_cols})
             VALUES
                 (:uaid, :channel_id, :endpoint, :scope, :key, :ctime, :app_server_key, :native_id)
             ON CONFLICT(uaid, channel_id) DO UPDATE SET
                 uaid = :uaid,
                 endpoint = :endpoint,
                 scope = :scope,
                 key = :key,
                 ctime = :ctime,
                 app_server_key = :app_server_key,
                 native_id = :native_id",
            common_cols = schema::COMMON_COLS,
        );
        let affected_rows = self.execute_named(
            &query,
            &[
                (":uaid", &record.uaid),
                (":channel_id", &record.channel_id),
                (":endpoint", &record.endpoint),
                (":scope", &record.scope),
                (":key", &record.key),
                (":ctime", &record.ctime),
                (":app_server_key", &record.app_server_key),
                (":native_id", &record.native_id),
            ],
        )?;
        Ok(affected_rows == 1)
    }

    fn delete_record(&self, uaid: &str, chid: &str) -> Result<bool> {
        let affected_rows = self.execute_named(
            "DELETE FROM push_record
             WHERE uaid = :uaid AND channel_id = :chid",
            &[(":uaid", &uaid), (":chid", &chid)],
        )?;
        Ok(affected_rows == 1)
    }

    fn delete_all_records(&self, uaid: &str) -> Result<()> {
        self.execute_named(
            "DELETE FROM push_record WHERE uaid = :uaid",
            &[(":uaid", &uaid)],
        )?;
        Ok(())
    }

    fn get_channel_list(&self, uaid: &str) -> Result<Vec<String>> {
        self.query_rows_and_then_named(
            "SELECT channel_id FROM push_record WHERE uaid = :uaid",
            &[(":uaid", &uaid)],
            |row| -> Result<String> { Ok(row.get_checked(0)?) },
        )
    }

    fn update_endpoint(&self, uaid: &str, channel_id: &str, endpoint: &str) -> Result<bool> {
        let affected_rows = self.execute_named(
            "UPDATE push_record set endpoint = :endpoint
             WHERE uaid = :uaid AND channel_id = :channel_id",
            &[
                (":endpoint", &endpoint),
                (":uaid", &uaid),
                (":channel_id", &channel_id),
            ],
        )?;
        Ok(affected_rows == 1)
    }

    fn update_native_id(&self, uaid: &str, native_id: &str) -> Result<bool> {
        let affected_rows = self.execute_named(
            "UPDATE push_record set native_id = :native_id WHERE uaid = :uaid",
            &[(":native_id", &native_id), (":uaid", &uaid)],
        )?;
        Ok(affected_rows == 1)
    }

    fn get_meta(&self, key: &str) -> Result<Option<String>> {
        // Get the most recent UAID (which should be the same value across all records,
        // but paranoia)
        let query = "SELECT value FROM meta_data where key = :key limit 1";
        let mut statement = self.prepare(query)?;
        let mut rows = statement.query_named(&[(":key", &key)])?;
        if let Some(row) = rows.next() {
            return Ok(Some(row?.get_checked("value")?));
        }
        Ok(None)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let query = "INSERT or REPLACE into meta_data (key, value) values (:k, :v)";
        self.execute_named(query, &[(":k", &key), (":v", &value)])?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crypto::{Crypto, Cryptography};
    use push_errors::Result;

    use super::PushDb;
    use crate::{db::Storage, record::PushRecord};

    const DUMMY_UAID: &str = "abad1dea00000000aabbccdd00000000";

    fn prec() -> PushRecord {
        PushRecord::new(
            DUMMY_UAID,
            "deadbeef00000000decafbad00000000",
            "https://example.com/update",
            "https://example.com/1",
            Crypto::generate_key().expect("Couldn't generate_key"),
        )
    }

    #[test]
    fn basic() -> Result<()> {
        let db = PushDb::open_in_memory()?;
        let rec = prec();
        let chid = &rec.channel_id;

        assert!(db.get_record(DUMMY_UAID, chid)?.is_none());
        assert!(db.put_record(&rec)?);
        assert!(db.get_record(DUMMY_UAID, chid)?.is_some());
        assert_eq!(db.get_record(DUMMY_UAID, chid)?, Some(rec.clone()));

        let mut rec2 = rec.clone();
        rec2.endpoint = "https://example.com/update2".to_owned();
        assert!(db.put_record(&rec2)?);
        let result = db.get_record(DUMMY_UAID, chid)?.unwrap();
        assert_ne!(result, rec);
        assert_eq!(result, rec2);
        Ok(())
    }

    #[test]
    fn delete() -> Result<()> {
        let db = PushDb::open_in_memory()?;
        let rec = prec();
        let chid = &rec.channel_id;

        assert!(db.put_record(&rec)?);
        assert!(db.get_record(DUMMY_UAID, chid)?.is_some());
        assert!(db.delete_record(DUMMY_UAID, chid)?);
        assert!(db.get_record(DUMMY_UAID, chid)?.is_none());
        Ok(())
    }

    #[test]
    fn delete_all_records() -> Result<()> {
        let db = PushDb::open_in_memory()?;
        let rec = prec();
        let mut rec2 = rec.clone();
        rec2.channel_id = "deadbeef00000002".to_owned();
        rec2.endpoint = "https://example.com/update2".to_owned();

        assert!(db.put_record(&rec)?);
        assert!(db.put_record(&rec2)?);
        assert!(db.get_record(DUMMY_UAID, &rec.channel_id)?.is_some());
        db.delete_all_records(DUMMY_UAID)?;
        assert!(db.get_record(DUMMY_UAID, &rec.channel_id)?.is_none());
        assert!(db.get_record(DUMMY_UAID, &rec.channel_id)?.is_none());
        Ok(())
    }

    #[test]
    fn meta() -> Result<()> {
        use super::Storage;
        let db = PushDb::open_in_memory()?;
        let no_rec = db.get_meta("uaid")?;
        assert_eq!(no_rec, None);
        db.set_meta("uaid", DUMMY_UAID)?;
        db.set_meta("fruit", "apple")?;
        db.set_meta("fruit", "banana")?;
        assert_eq!(db.get_meta("uaid")?, Some(DUMMY_UAID.to_owned()));
        assert_eq!(db.get_meta("fruit")?, Some("banana".to_owned()));
        Ok(())
    }
}
