/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::result;
use std::cell::Cell;
use std::ops::Deref;
use error::*;
use failure;
use rusqlite::{Connection};
use rusqlite::{types::{ToSql, FromSql}};
use sql_support::{ConnExt};
use sync15_adapter::{
    ClientInfo,
    IncomingChangeset,
    KeyBundle,
    OutgoingChangeset,
    Store,
    ServerTimestamp,
    Sync15StorageClientInit,
    sync_multiple,
};
use sync15_adapter::request::{CollectionRequest};

use super::plan::{apply_plan};
use super::{MAX_INCOMING_PLACES};

static LAST_SYNC_META_KEY:    &'static str = "history_last_sync_time";
static GLOBAL_STATE_META_KEY: &'static str = "history_global_state";

// Lifetime here seems wrong
pub struct HistoryStore<'a> {
    pub db: &'a Connection,
    pub client_info: Cell<Option<ClientInfo>>,
}

impl<'a> HistoryStore<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db, client_info: Cell::new(None) }
    }

    fn put_meta(&self, key: &str, value: &ToSql) -> Result<()> {
        self.execute_named_cached(
            "REPLACE INTO moz_meta (key, value) VALUES (:key, :value)",
            &[(":key", &key as &ToSql), (":value", value)]
        )?;
        Ok(())
    }

    fn get_meta<T: FromSql>(&self, key: &str) -> Result<Option<T>> {
        Ok(self.try_query_row(
            "SELECT value FROM moz_meta WHERE key = :key",
            &[(":key", &key as &ToSql)],
            |row| Ok::<_, Error>(row.get_checked(0)?),
            true
        )?)
    }

    fn do_apply_incoming(
        &self,
        inbound: IncomingChangeset
    ) -> Result<OutgoingChangeset> {
        apply_plan(&self, inbound)
    }

    fn set_global_state(&self, global_state: Option<String>) -> Result<()> {
        let to_write = match global_state {
            Some(ref s) => s,
            None => "",
        };
        self.put_meta(GLOBAL_STATE_META_KEY, &to_write)
    }

    fn get_global_state(&self) -> Result<Option<String>> {
        self.get_meta::<String>(GLOBAL_STATE_META_KEY)
    }

    /// A convenience wrapper around sync_multiple.
    pub fn sync(&self,
                storage_init: &Sync15StorageClientInit,
                root_sync_key: &KeyBundle) -> Result<()> {
        let global_state: Cell<Option<String>> = Cell::new(self.get_global_state()?);
        let result = sync_multiple(&[self],
                                   &global_state,
                                   &self.client_info,
                                   storage_init,
                                   root_sync_key);
        self.set_global_state(global_state.replace(None))?;
        let failures = result?;
        if failures.len() == 0 {
            Ok(())
        } else {
            assert_eq!(failures.len(), 1);
            let (name, err) = failures.into_iter().next().unwrap();
            assert_eq!(name, "history");
            Err(err.into())
        }
    }
}


impl<'a> ConnExt for HistoryStore<'a> {
    #[inline]
    fn conn(&self) -> &Connection {
        &self.db
    }
}

impl<'a> Deref for HistoryStore<'a> {
    type Target = Connection;
    #[inline]
    fn deref(&self) -> &Connection {
        &self.db
    }
}

impl<'a> Store for HistoryStore<'a> {
    fn collection_name(&self) -> &'static str {
        "history"
    }

    fn apply_incoming(
        &self,
        inbound: IncomingChangeset
    ) -> result::Result<OutgoingChangeset, failure::Error> {
        Ok(self.do_apply_incoming(inbound)?)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> result::Result<(), failure::Error> {
        println!("sync completed {} records, should advance timestamp to {}",
                 records_synced.len(), new_timestamp);
        Ok(())
    }

    fn get_collection_request(&self) -> result::Result<CollectionRequest, failure::Error> {
        let since = self.get_meta::<i64>(LAST_SYNC_META_KEY)?
                        .map(|millis| ServerTimestamp(millis as f64 / 1000.0))
                        .unwrap_or_default();
        Ok(CollectionRequest::new("history").full().newer_than(since).limit(MAX_INCOMING_PLACES))
    }

    fn reset(&self) -> result::Result<(), failure::Error> {
        warn!("reset not implemented");
        Ok(())
    }

    fn wipe(&self) -> result::Result<(), failure::Error> {
        warn!("not implemented");
        Ok(())
    }
}
