/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use std::{ops::Deref, path::Path};

use rusqlite::Connection;
use sql_support::{open_database, ConnExt};

use crate::error::{debug, PushError, Result};

use super::{record::PushRecord, schema};

pub trait Storage: Sized {
    fn open<P: AsRef<Path>>(path: P) -> Result<Self>;

    fn get_record(&self, chid: &str) -> Result<Option<PushRecord>>;

    fn get_record_by_scope(&self, scope: &str) -> Result<Option<PushRecord>>;

    fn put_record(&self, record: &PushRecord) -> Result<bool>;

    fn delete_record(&self, chid: &str) -> Result<bool>;

    fn delete_all_records(&self) -> Result<()>;

    fn get_channel_list(&self) -> Result<Vec<String>>;

    #[allow(dead_code)]
    fn update_endpoint(&self, channel_id: &str, endpoint: &str) -> Result<bool>;

    // Some of our "meta" keys are more important than others, so they get special helpers.
    fn get_uaid(&self) -> Result<Option<String>>;
    fn set_uaid(&self, uaid: &str) -> Result<()>;

    fn get_auth(&self) -> Result<Option<String>>;
    fn set_auth(&self, auth: &str) -> Result<()>;

    fn get_registration_id(&self) -> Result<Option<String>>;
    fn set_registration_id(&self, native_id: &str) -> Result<()>;

    // And general purpose meta with hard-coded key names spread everywhere.
    fn get_meta(&self, key: &str) -> Result<Option<String>>;
    fn set_meta(&self, key: &str, value: &str) -> Result<()>;
}

pub struct PushDb {
    pub db: Connection,
}

impl PushDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        // By default, file open errors are StorageSqlErrors and aren't super helpful.
        // Instead, remap to StorageError and provide the path to the file that couldn't be opened.
        let initializer = schema::PushConnectionInitializer {};
        let db = open_database::open_database(path, &initializer).map_err(|orig| {
            PushError::StorageError(format!(
                "Could not open database file {:?} - {}",
                &path.as_os_str(),
                orig,
            ))
        })?;
        Ok(Self { db })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        // A nod to our tests which use this.
        error_support::init_for_tests();

        let initializer = schema::PushConnectionInitializer {};
        let db = open_database::open_memory_database(&initializer)?;
        Ok(Self { db })
    }

    /// Normalize UUID values to undashed, lowercase.
    // The server mangles ChannelID UUIDs to undashed lowercase values. We should force those
    // so that key lookups continue to work.
    pub fn normalize_uuid(uuid: &str) -> String {
        uuid.replace('-', "").to_lowercase()
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
    fn get_record(&self, chid: &str) -> Result<Option<PushRecord>> {
        let query = format!(
            "SELECT {common_cols}
             FROM push_record WHERE channel_id = :chid",
            common_cols = schema::COMMON_COLS,
        );
        self.try_query_row(
            &query,
            &[(":chid", &Self::normalize_uuid(chid))],
            PushRecord::from_row,
            false,
        )
    }

    fn get_record_by_scope(&self, scope: &str) -> Result<Option<PushRecord>> {
        let query = format!(
            "SELECT {common_cols}
             FROM push_record WHERE scope = :scope",
            common_cols = schema::COMMON_COLS,
        );
        self.try_query_row(&query, &[(":scope", scope)], PushRecord::from_row, false)
    }

    fn put_record(&self, record: &PushRecord) -> Result<bool> {
        debug!(
            "adding push subscription for scope '{}', channel '{}', endpoint '{}'",
            record.scope, record.channel_id, record.endpoint
        );
        let query = format!(
            "INSERT OR REPLACE INTO push_record
                 ({common_cols})
             VALUES
                 (:channel_id, :endpoint, :scope, :key, :ctime, :app_server_key)",
            common_cols = schema::COMMON_COLS,
        );
        let affected_rows = self.execute(
            &query,
            &[
                (
                    ":channel_id",
                    &Self::normalize_uuid(&record.channel_id) as &dyn rusqlite::ToSql,
                ),
                (":endpoint", &record.endpoint),
                (":scope", &record.scope),
                (":key", &record.key),
                (":ctime", &record.ctime),
                (":app_server_key", &record.app_server_key),
            ],
        )?;
        Ok(affected_rows == 1)
    }

    fn delete_record(&self, chid: &str) -> Result<bool> {
        debug!("deleting push subscription: {}", chid);
        let affected_rows = self.execute(
            "DELETE FROM push_record
             WHERE channel_id = :chid",
            &[(":chid", &Self::normalize_uuid(chid))],
        )?;
        Ok(affected_rows == 1)
    }

    fn delete_all_records(&self) -> Result<()> {
        debug!("deleting all push subscriptions and some metadata");
        self.execute("DELETE FROM push_record", [])?;
        // Clean up the meta data records as well, since we probably want to reset the
        // UAID and get a new secret.
        // Note we *do not* delete the registration_id - it's possible we are deleting all
        // subscriptions because we just provided a different registration_id.
        self.execute_batch(
            "DELETE FROM meta_data WHERE key='uaid';
             DELETE FROM meta_data WHERE key='auth';
             ",
        )?;
        Ok(())
    }

    fn get_channel_list(&self) -> Result<Vec<String>> {
        self.query_rows_and_then(
            "SELECT channel_id FROM push_record",
            [],
            |row| -> Result<String> { Ok(row.get(0)?) },
        )
    }

    fn update_endpoint(&self, channel_id: &str, endpoint: &str) -> Result<bool> {
        debug!("updating endpoint for '{}' to '{}'", channel_id, endpoint);
        let affected_rows = self.execute(
            "UPDATE push_record set endpoint = :endpoint
             WHERE channel_id = :channel_id",
            &[
                (":endpoint", &endpoint as &dyn rusqlite::ToSql),
                (":channel_id", &Self::normalize_uuid(channel_id)),
            ],
        )?;
        Ok(affected_rows == 1)
    }

    // A couple of helpers to get/set "well known" meta keys.
    fn get_uaid(&self) -> Result<Option<String>> {
        self.get_meta("uaid")
    }

    fn set_uaid(&self, uaid: &str) -> Result<()> {
        self.set_meta("uaid", uaid)
    }

    fn get_auth(&self) -> Result<Option<String>> {
        self.get_meta("auth")
    }

    fn set_auth(&self, auth: &str) -> Result<()> {
        self.set_meta("auth", auth)
    }

    fn get_registration_id(&self) -> Result<Option<String>> {
        self.get_meta("registration_id")
    }

    fn set_registration_id(&self, registration_id: &str) -> Result<()> {
        self.set_meta("registration_id", registration_id)
    }

    fn get_meta(&self, key: &str) -> Result<Option<String>> {
        // Get the most recent UAID (which should be the same value across all records,
        // but paranoia)
        self.try_query_one(
            "SELECT value FROM meta_data where key = :key limit 1",
            &[(":key", &key)],
            true,
        )
        .map_err(PushError::StorageSqlError)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let query = "INSERT or REPLACE into meta_data (key, value) values (:k, :v)";
        self.execute_cached(query, &[(":k", &key), (":v", &value)])?;
        Ok(())
    }

    #[cfg(not(test))]
    fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        PushDb::open(path)
    }

    #[cfg(test)]
    fn open<P: AsRef<Path>>(_path: P) -> Result<Self> {
        PushDb::open_in_memory()
    }
}

#[cfg(test)]
mod test {
    use crate::error::Result;
    use crate::internal::crypto::{Crypto, Cryptography};

    use super::PushDb;
    use crate::internal::crypto::get_random_bytes;
    use crate::internal::storage::{db::Storage, record::PushRecord};
    use nss::ensure_initialized;

    const DUMMY_UAID: &str = "abad1dea00000000aabbccdd00000000";

    fn get_db() -> Result<PushDb> {
        error_support::init_for_tests();
        // NOTE: In Memory tests can sometimes produce false positives. Use the following
        // for debugging
        // PushDb::open("/tmp/push.sqlite");
        PushDb::open_in_memory()
    }

    fn get_uuid() -> Result<String> {
        Ok(get_random_bytes(16)?
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<String>>()
            .join(""))
    }

    fn prec(chid: &str) -> PushRecord {
        PushRecord::new(
            chid,
            &format!("https://example.com/update/{}", chid),
            "https://example.com/",
            Crypto::generate_key().expect("Couldn't generate_key"),
        )
        .unwrap()
    }

    #[test]
    fn basic() -> Result<()> {
        ensure_initialized();

        let db = get_db()?;
        let chid = &get_uuid()?;
        let rec = prec(chid);

        assert!(db.get_record(chid)?.is_none());
        db.put_record(&rec)?;
        assert!(db.get_record(chid)?.is_some());
        // don't fail if you've already added this record.
        db.put_record(&rec)?;
        // make sure that fetching the same uaid & chid returns the same record.
        assert_eq!(db.get_record(chid)?, Some(rec.clone()));

        let mut rec2 = rec.clone();
        rec2.endpoint = format!("https://example.com/update2/{}", chid);
        db.put_record(&rec2)?;
        let result = db.get_record(chid)?.unwrap();
        assert_ne!(result, rec);
        assert_eq!(result, rec2);

        let result = db.get_record_by_scope("https://example.com/")?.unwrap();
        assert_eq!(result, rec2);

        Ok(())
    }

    #[test]
    fn delete() -> Result<()> {
        ensure_initialized();

        let db = get_db()?;
        let chid = &get_uuid()?;
        let rec = prec(chid);

        assert!(db.put_record(&rec)?);
        assert!(db.get_record(chid)?.is_some());
        assert!(db.delete_record(chid)?);
        assert!(db.get_record(chid)?.is_none());
        Ok(())
    }

    #[test]
    fn delete_all_records() -> Result<()> {
        ensure_initialized();

        let db = get_db()?;
        let chid = &get_uuid()?;
        let rec = prec(chid);
        let mut rec2 = rec.clone();
        rec2.channel_id = get_uuid()?;
        rec2.endpoint = format!("https://example.com/update/{}", &rec2.channel_id);

        assert!(db.put_record(&rec)?);
        // save a record with different channel and endpoint, but same scope - it should overwrite
        // the first because scopes are unique.
        assert!(db.put_record(&rec2)?);
        assert!(db.get_record(&rec.channel_id)?.is_none());
        assert!(db.get_record(&rec2.channel_id)?.is_some());
        db.delete_all_records()?;
        assert!(db.get_record(&rec.channel_id)?.is_none());
        assert!(db.get_record(&rec.channel_id)?.is_none());
        assert!(db.get_uaid()?.is_none());
        assert!(db.get_auth()?.is_none());
        Ok(())
    }

    #[test]
    fn meta() -> Result<()> {
        ensure_initialized();

        use super::Storage;
        let db = get_db()?;
        let no_rec = db.get_uaid()?;
        assert_eq!(no_rec, None);
        db.set_uaid(DUMMY_UAID)?;
        db.set_meta("fruit", "apple")?;
        db.set_meta("fruit", "banana")?;
        assert_eq!(db.get_uaid()?, Some(DUMMY_UAID.to_owned()));
        assert_eq!(db.get_meta("fruit")?, Some("banana".to_owned()));
        Ok(())
    }

    #[test]
    fn dash() -> Result<()> {
        ensure_initialized();

        let db = get_db()?;
        let chid = "deadbeef-0000-0000-0000-decafbad12345678";

        let rec = prec(chid);

        assert!(db.put_record(&rec)?);
        assert!(db.get_record(chid)?.is_some());
        assert!(db.delete_record(chid)?);
        Ok(())
    }
}
