use std::{ops::Deref, path::Path};

use rusqlite::{types::ToSql, Connection, NO_PARAMS};
use sql_support::ConnExt;

use crate::{
    error::{Error, Result},
    record::PushRecord,
    schema,
};

// TODO: Add broadcasts storage

pub trait Storage {
    fn get_record(&self, uaid: &str, chid: &str) -> Result<Option<PushRecord>>;

    fn put_record(&self, uaid: &str, record: &PushRecord) -> Result<bool>;

    fn delete_record(&self, uaid: &str, chid: &str) -> Result<bool>;

    fn delete_all_records(&self, uaid: &str) -> Result<()>;
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

    pub fn open(path: impl AsRef<Path>) -> Result<impl Storage> {
        Ok(Self::with_connection(Connection::open(path)?)?)
    }

    fn open_in_memory() -> Result<impl Storage> {
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
        *&self
    }
}

impl Storage for PushDb {
    fn get_record(&self, _uaid: &str, chid: &str) -> Result<Option<PushRecord>> {
        let query = format!(
            "SELECT {common_cols}
             FROM push_record WHERE channel_id = :chid",
            common_cols = schema::COMMON_COLS,
        );
        Ok(self.try_query_row(
            &query,
            &[(":chid", &chid as &ToSql)],
            PushRecord::from_row,
            false,
        )?)
    }

    fn put_record(&self, _uaid: &str, record: &PushRecord) -> Result<bool> {
        let query = format!(
            "INSERT INTO push_record
                 ({common_cols})
             VALUES
                 (:channel_id, :endpoint, :scope, :origin_attributes, :key, :system_record,
                  :recent_message_ids, :push_count, :last_push, :ctime, :quota, :app_server_key,
                  :native_id)
             ON CONFLICT(channel_id) DO UPDATE SET
                 endpoint = :endpoint,
                 scope = :scope,
                 origin_attributes = :origin_attributes,
                 key = :key,
                 system_record = :system_record,
                 recent_message_ids = :recent_message_ids,
                 push_count = :push_count,
                 last_push = :last_push,
                 ctime = :ctime,
                 quota = :quota,
                 app_server_key = :app_server_key,
                 native_id = :native_id",
            common_cols = schema::COMMON_COLS,
        );
        let affected_rows = self.execute_named(
            &query,
            &[
                (":channel_id", &record.channel_id),
                (":endpoint", &record.endpoint),
                (":scope", &record.scope),
                (":origin_attributes", &record.origin_attributes),
                (":key", &record.key),
                (":system_record", &record.system_record),
                (
                    ":recent_message_ids",
                    &serde_json::to_string(&record.recent_message_ids).map_err(|e| {
                        Error::internal(&format!("Serializing recent_message_ids: {}", e))
                    })?,
                ),
                (":push_count", &record.push_count),
                (":last_push", &record.last_push),
                (":ctime", &record.ctime),
                (":quota", &record.quota),
                (":app_server_key", &record.app_server_key),
                (":native_id", &record.native_id),
            ],
        )?;
        Ok(affected_rows == 1)
    }

    fn delete_record(&self, _uaid: &str, chid: &str) -> Result<bool> {
        let affected_rows = self.execute_named(
            "DELETE FROM push_record
             WHERE channel_id = :chid",
            &[(":chid", &chid as &ToSql)],
        )?;
        Ok(affected_rows == 1)
    }

    fn delete_all_records(&self, _uaid: &str) -> Result<()> {
        self.execute("DELETE FROM push_record", NO_PARAMS)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::PushDb;
    use crate::{db::Storage, error::Result, record::PushRecord};
    use crypto::{Crypto, Cryptography};

    fn prec() -> PushRecord {
        PushRecord::new(
            "",
            "deadbeef00000000decafbad00000000",
            "https://example.com/update",
            "https://example.com/1",
            "appId=1",
            Crypto::generate_key().expect("Couldn't generate_key"),
            false,
        )
    }

    #[test]
    fn basic() -> Result<()> {
        let db = PushDb::open_in_memory()?;
        let rec = prec();
        let chid = &rec.channel_id;

        assert!(db.get_record("", chid)?.is_none());

        assert!(db.put_record("", &rec)?);
        assert!(db.get_record("", chid)?.is_some());
        assert_eq!(db.get_record("", chid)?, Some(rec.clone()));

        let mut rec2 = rec.clone();
        rec2.increment()?;
        assert!(db.put_record("", &rec2)?);
        let result = db.get_record("", chid)?.unwrap();
        assert_ne!(result, rec);
        assert_eq!(result, rec2);
        Ok(())
    }

    #[test]
    fn delete() -> Result<()> {
        let db = PushDb::open_in_memory()?;
        let rec = prec();
        let chid = &rec.channel_id;

        assert!(db.put_record("", &rec)?);
        assert!(db.get_record("", chid)?.is_some());
        assert!(db.delete_record("", chid)?);
        assert!(db.get_record("", chid)?.is_none());
        Ok(())
    }

    #[test]
    fn delete_all_records() -> Result<()> {
        let db = PushDb::open_in_memory()?;
        let rec = prec();
        let mut rec2 = rec.clone();
        rec2.channel_id = "deadbeef00000002".to_owned();
        rec2.endpoint = "https://example.com/update2".to_owned();

        assert!(db.put_record("", &rec)?);
        assert!(db.put_record("", &rec2)?);
        assert!(db.get_record("", &rec.channel_id)?.is_some());
        db.delete_all_records("")?;
        assert!(db.get_record("", &rec.channel_id)?.is_none());
        assert!(db.get_record("", &rec2.channel_id)?.is_none());
        Ok(())
    }
}
