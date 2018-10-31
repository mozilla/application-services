/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::result;
use std::cell::RefCell;
use std::ops::Deref;
use error::*;
use failure;
use rusqlite::{Connection};
use rusqlite::{types::{ToSql, FromSql}};
use serde_json;
use url::{Url};

use types::{SyncGuid, Timestamp};
use sql_support::{ConnExt};
use sync15_adapter::{GlobalState, IncomingChangeset, OutgoingChangeset, Store, ServerTimestamp};
use sync15_adapter::driver::{GlobalStateProvider, ClientInfo};

use super::record::{HistorySyncRecord, HistoryRecord};
use super::super::super::api::history::{can_add_url};
use super::super::super::storage::{fetch_visits};

static LAST_SYNC_META_KEY:    &'static str = "history_last_sync_time";
static GLOBAL_STATE_META_KEY: &'static str = "history_global_state";

#[derive(Debug)]
enum IncomingPlan {
    Skip, // An entry we just want to ignore.
    Invalid(SyncGuid, Error), // Something's wrong with this entry.
    Failed(SyncGuid, Error), // The entry appears sane, but there was some error.
    Delete(SyncGuid), // We should delete this.
    Whatever(SyncGuid),
}

// Lifetime here seems wrong
pub struct HistoryStore<'a> {
    pub db: &'a Connection,
    pub client_info: RefCell<Option<ClientInfo>>,
}

impl<'a> HistoryStore<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db, client_info: RefCell::new(None) }
    }
    /// Clamps a history visit date between the current date and the earliest
    /// sensible date.
    fn clamp_visit_date(visit_date: Timestamp) -> Timestamp {
        let now = Timestamp::now();
        if visit_date > now {
            return visit_date;
        }
        if visit_date < *EARLIEST_BOOKMARK_TIMESTAMP {
            return *EARLIEST_BOOKMARK_TIMESTAMP;
        }
        return visit_date;
    }

    fn process_record(&self, record: HistoryRecord) -> IncomingPlan {
        let url = match Url::parse(&record.hist_uri) {
            Ok(u) => u,
            Err(e) => return IncomingPlan::Invalid(record.id.clone(), e.into()),
        };
        match can_add_url(&url) {
            Ok(can) => if !can { return IncomingPlan::Skip },
            Err(e) => return IncomingPlan::Failed(record.id.clone(), e.into()),
        }
        // Let's get what we know about it, if anything - last 20, like desktop?
        let visits = match fetch_visits(self.db, &url, 20) {
            Ok(v) => v,
            Err(e) => return IncomingPlan::Failed(record.id.clone(), e.into()),
        };
        println!("url {} has visits {:?}", url, visits);

        for visit in visits {
            println!("visit at {}", visit.visit_date);
        }

        IncomingPlan::Whatever(record.id)
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
        // for a first-cut, let's do this in the most naive way possible...
        for incoming in inbound.changes.iter() {
            let item = HistorySyncRecord::from_payload(incoming.0.clone())?;
            let plan = match item.record {
                Some(record) => self.process_record(record),
                None => IncomingPlan::Delete(item.guid.clone()),
            };
            println!("incoming {:?} -> {:?}", &item.guid, &plan)
        }
        let outgoing = OutgoingChangeset::new("history".into(), inbound.timestamp);
        Ok(outgoing)
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
    fn collection_name(&self) -> String {
        "history".into()
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
        println!("sync done!");
        Ok(())
        // Ok(self.mark_as_synchronized(
        //     &records_synced.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
        //     new_timestamp
        // )?)
    }

    fn get_last_sync(&self) -> result::Result<Option<ServerTimestamp>, failure::Error> {
        Ok(self.get_meta::<i64>(LAST_SYNC_META_KEY)?
            .map(|millis| ServerTimestamp(millis as f64 / 1000.0)))
    }

    fn reset(&self) -> result::Result<(), failure::Error> {
        warn!("reset not implemented");
        Ok(())
    }

    fn wipe(&self) -> result::Result<(), failure::Error> {
        panic!("not implemented");
        Ok(())
    }
}

// XXX - lots of serde stuff cloned from logins - should share this?
// State provider really boils down to storage of a single string!
impl<'a> GlobalStateProvider for HistoryStore<'a> {
    fn load(&self) -> result::Result<Option<GlobalState>, failure::Error> {
        Ok(match self.get_meta::<String>(GLOBAL_STATE_META_KEY)? {
            Some(persisted_global_state) => {
                match serde_json::from_str::<GlobalState>(&persisted_global_state) {
                    Ok(state) => Some(state),
                    _ => {
                        // Don't log the error since it might contain sensitive
                        // info like keys (the JSON does, after all).
                        error!("Failed to parse GlobalState from JSON! Falling back to default");
                        None
                    }
                }
            },
            None => None
        })
    }

    fn save(&self, maybe_state: Option<&GlobalState>) -> result::Result<(), failure::Error> {
        info!("Updating persisted global state");
        let s: String = match maybe_state {
            Some(state) => {
                state.to_persistable_string()
            },
            None => "".to_string(),
        };
        self.put_meta(GLOBAL_STATE_META_KEY, &s)?;
        Ok(())
    }

    fn swap_client_info(&self, new_info: Option<ClientInfo>) -> result::Result<Option<ClientInfo>, failure::Error> {
        Ok(self.client_info.replace(new_info))
    }
}

lazy_static! {
    // extern crate chrono;
    // use chrono::{NaiveDate, NaiveDateTime};

    static ref EARLIEST_BOOKMARK_TIMESTAMP: Timestamp = Timestamp(0); // XXX - want Jan 23, 1993
    // let date_time: NaiveDateTime = NaiveDate::from_ymd(2017, 11, 12)
}
