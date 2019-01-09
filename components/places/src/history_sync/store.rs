/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::storage::history::history_sync::reset_storage;
use rusqlite::types::{FromSql, ToSql};
use rusqlite::Connection;
use sql_support::ConnExt;
use std::cell::Cell;
use std::ops::Deref;
use std::result;
use sync15::request::CollectionRequest;
use sync15::telemetry;
use sync15::{
    sync_multiple, ClientInfo, IncomingChangeset, KeyBundle, OutgoingChangeset, ServerTimestamp,
    Store, Sync15StorageClientInit,
};

use super::plan::{apply_plan, finish_plan};
use super::MAX_INCOMING_PLACES;

static LAST_SYNC_META_KEY: &'static str = "history_last_sync_time";
static GLOBAL_STATE_META_KEY: &'static str = "history_global_state";

// Lifetime here seems wrong
pub struct HistoryStore<'a> {
    pub db: &'a Connection,
    pub client_info: Cell<Option<ClientInfo>>,
}

impl<'a> HistoryStore<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self {
            db,
            client_info: Cell::new(None),
        }
    }

    fn put_meta(&self, key: &str, value: &ToSql) -> Result<()> {
        self.execute_named_cached(
            "REPLACE INTO moz_meta (key, value) VALUES (:key, :value)",
            &[(":key", &key as &ToSql), (":value", value)],
        )?;
        Ok(())
    }

    fn get_meta<T: FromSql>(&self, key: &str) -> Result<Option<T>> {
        Ok(self.try_query_row(
            "SELECT value FROM moz_meta WHERE key = :key",
            &[(":key", &key as &ToSql)],
            |row| Ok::<_, Error>(row.get_checked(0)?),
            true,
        )?)
    }

    fn do_apply_incoming(
        &self,
        inbound: IncomingChangeset,
        incoming_telemetry: &mut telemetry::EngineIncoming,
    ) -> Result<OutgoingChangeset> {
        let timestamp = inbound.timestamp;
        let outgoing = apply_plan(&self, inbound, incoming_telemetry)?;
        // write the timestamp now, so if we are interrupted creating outgoing
        // changesets we don't need to re-reconcile what we just did.
        self.put_meta(LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;
        Ok(outgoing)
    }

    fn do_sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> Result<()> {
        log::info!(
            "sync completed after uploading {} records",
            records_synced.len()
        );
        finish_plan(&self.db)?;

        // write timestamp to reflect what we just wrote.
        self.put_meta(LAST_SYNC_META_KEY, &(new_timestamp.as_millis() as i64))?;

        Ok(())
    }

    fn do_reset(&self) -> Result<()> {
        log::info!("Resetting history store");
        reset_storage(self.db)?;
        self.put_meta(LAST_SYNC_META_KEY, &0)?;
        Ok(())
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
    pub fn sync(
        &self,
        storage_init: &Sync15StorageClientInit,
        root_sync_key: &KeyBundle,
        sync_ping: &mut telemetry::SyncTelemetryPing,
    ) -> Result<()> {
        let global_state: Cell<Option<String>> = Cell::new(self.get_global_state()?);
        let result = sync_multiple(
            &[self],
            &global_state,
            &self.client_info,
            storage_init,
            root_sync_key,
            sync_ping,
        );
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
        inbound: IncomingChangeset,
        incoming_telemetry: &mut telemetry::EngineIncoming,
    ) -> result::Result<OutgoingChangeset, failure::Error> {
        Ok(self.do_apply_incoming(inbound, incoming_telemetry)?)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> result::Result<(), failure::Error> {
        Ok(self.do_sync_finished(new_timestamp, records_synced)?)
    }

    fn get_collection_request(&self) -> result::Result<CollectionRequest, failure::Error> {
        let since = self
            .get_meta::<i64>(LAST_SYNC_META_KEY)?
            .map(|millis| ServerTimestamp(millis as f64 / 1000.0))
            .unwrap_or_default();
        Ok(CollectionRequest::new("history")
            .full()
            .newer_than(since)
            .limit(MAX_INCOMING_PLACES))
    }

    fn reset(&self) -> result::Result<(), failure::Error> {
        self.do_reset()?;
        Ok(())
    }

    fn wipe(&self) -> result::Result<(), failure::Error> {
        log::warn!("not implemented");
        Ok(())
    }
}
