/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{plan_incoming, ProcessIncomingRecordImpl, SyncRecord};
use crate::db::AutofillDb;
use crate::error::*;
use rusqlite::{
    types::{FromSql, ToSql},
    Connection, Transaction,
};
use std::sync::{Arc, Mutex};
use sync15::{
    telemetry, CollSyncIds, CollectionRequest, EngineSyncAssociation, IncomingChangeset,
    OutgoingChangeset, ServerTimestamp, SyncEngine,
};
use sync_guid::Guid;

// We have 2 engines in this crate and they are identical except for stuff
// abstracted here!
pub struct EngineConfig {
    pub(crate) namespace: String,        // prefix for meta keys, etc.
    pub(crate) collection: &'static str, // static collection name on the server.
}

// meta keys, will be prefixed by the "namespace"
pub const LAST_SYNC_META_KEY: &str = "last_sync_time";
pub const GLOBAL_SYNCID_META_KEY: &str = "global_sync_id";
pub const COLLECTION_SYNCID_META_KEY: &str = "sync_id";

// A trait to abstract the broader sync processes.
pub trait SyncEngineStorageImpl<T> {
    fn get_incoming_impl(&self) -> Box<dyn ProcessIncomingRecordImpl<Record = T>>;
    fn reset_storage(&self, conn: &Transaction<'_>) -> Result<()>;
}

// A sync engine that gets functionality from an EngineConfig.
pub struct ConfigSyncEngine<T> {
    pub(crate) config: EngineConfig,
    pub(crate) db: Arc<Mutex<AutofillDb>>,
    pub(crate) storage_impl: Box<dyn SyncEngineStorageImpl<T>>,
}

impl<T> ConfigSyncEngine<T> {
    fn put_meta(&self, conn: &Connection, tail: &str, value: &dyn ToSql) -> Result<()> {
        let key = format!("{}.{}", self.config.namespace, tail);
        crate::db::store::put_meta(conn, &key, value)
    }
    fn get_meta<V: FromSql>(&self, conn: &Connection, tail: &str) -> Result<Option<V>> {
        let key = format!("{}.{}", self.config.namespace, tail);
        crate::db::store::get_meta(conn, &key)
    }
    fn delete_meta(&self, conn: &Connection, tail: &str) -> Result<()> {
        let key = format!("{}.{}", self.config.namespace, tail);
        crate::db::store::delete_meta(conn, &key)
    }
}

// We're just an "adaptor" to the sync15 version of an 'engine'
impl<T: SyncRecord + std::fmt::Debug> SyncEngine for ConfigSyncEngine<T> {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        self.config.collection.into()
    }

    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> anyhow::Result<OutgoingChangeset> {
        assert_eq!(inbound.len(), 1, "we only request one item");
        let inbound = inbound.into_iter().next().unwrap();

        let db = self.db.lock().unwrap();
        crate::db::schema::create_empty_sync_temp_tables(&db.writer)?;

        let signal = db.begin_interrupt_scope();

        // Stage all incoming items.
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let timestamp = inbound.timestamp;
        let num_incoming = inbound.changes.len() as u32;
        let tx = db.writer.unchecked_transaction()?;
        let incoming_impl = self.storage_impl.get_incoming_impl();

        // The first step in the "apply incoming" process for syncing autofill records.
        incoming_impl.stage_incoming(&tx, inbound.changes, &signal)?;
        // 2nd step is to get "states" for each record...
        for state in incoming_impl.fetch_incoming_states(&tx)? {
            signal.err_if_interrupted()?;
            // Finally get a "plan" and apply it.
            let action = plan_incoming(&*incoming_impl, &tx, state)?;
            super::apply_incoming_action(&*incoming_impl, &tx, action)?;
        }
        incoming_telemetry.applied(num_incoming);
        telem.incoming(incoming_telemetry);

        // write the timestamp now, so if we are interrupted merging or
        // creating outgoing changesets we don't need to re-download the same
        // records.
        self.put_meta(&tx, LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;
        // Not quite sure if we should commit now and then stage outgoing?
        tx.commit()?;

        // Finally, stage outgoing items.
        // TODO: Call yet-to-be-implemented stage outgoing code
        //     let outgoing = self.fetch_outgoing_records(timestamp)?;
        //     Ok(outgoing)
        Ok(OutgoingChangeset::new(self.collection_name(), timestamp))
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        _records_synced: Vec<Guid>,
    ) -> anyhow::Result<()> {
        let db = self.db.lock().unwrap();
        self.put_meta(
            &db.writer,
            LAST_SYNC_META_KEY,
            &(new_timestamp.as_millis() as i64),
        )?;
        // TODO: Call yet-to-be implement stage outgoing code
        db.pragma_update(None, "wal_checkpoint", &"PASSIVE")?;
        Ok(())
    }

    fn get_collection_requests(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Vec<CollectionRequest>> {
        let db = self.db.lock().unwrap();
        let since = ServerTimestamp(
            self.get_meta::<i64>(&db.writer, LAST_SYNC_META_KEY)?
                .unwrap_or_default(),
        );
        Ok(if since == server_timestamp {
            vec![]
        } else {
            vec![CollectionRequest::new(self.collection_name())
                .full()
                .newer_than(since)]
        })
    }

    fn get_sync_assoc(&self) -> anyhow::Result<EngineSyncAssociation> {
        let db = self.db.lock().unwrap();
        let global = self.get_meta(&db.writer, GLOBAL_SYNCID_META_KEY)?;
        let coll = self.get_meta(&db.writer, COLLECTION_SYNCID_META_KEY)?;
        Ok(if let (Some(global), Some(coll)) = (global, coll) {
            EngineSyncAssociation::Connected(CollSyncIds { global, coll })
        } else {
            EngineSyncAssociation::Disconnected
        })
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> anyhow::Result<()> {
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        self.storage_impl.reset_storage(&tx)?;
        // Reset the last sync time, so that the next sync fetches fresh records
        // from the server.
        self.put_meta(&tx, LAST_SYNC_META_KEY, &0)?;

        // Clear the sync ID if we're signing out, or set it to whatever the
        // server gave us if we're signing in.
        match assoc {
            EngineSyncAssociation::Disconnected => {
                self.delete_meta(&tx, GLOBAL_SYNCID_META_KEY)?;
                self.delete_meta(&tx, COLLECTION_SYNCID_META_KEY)?;
            }
            EngineSyncAssociation::Connected(ids) => {
                self.put_meta(&tx, GLOBAL_SYNCID_META_KEY, &ids.global)?;
                self.put_meta(&tx, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    fn wipe(&self) -> anyhow::Result<()> {
        unimplemented!("no caller 'cos there isn't a valid use case for it");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::credit_cards::add_internal_credit_card;
    use crate::db::credit_cards::tests::{get_all, insert_mirror_record, insert_tombstone_record};
    use crate::db::models::credit_card::InternalCreditCard;
    use crate::db::test::new_mem_db;
    use rusqlite::NO_PARAMS;
    use sql_support::ConnExt;

    // We use the credit-card engine here.
    fn create_engine(db: AutofillDb) -> ConfigSyncEngine<InternalCreditCard> {
        crate::sync::credit_card::create_engine(Arc::new(Mutex::new(db)))
    }

    pub fn clear_cc_tables(conn: &Connection) -> rusqlite::Result<(), rusqlite::Error> {
        conn.execute_all(&[
            "DELETE FROM credit_cards_data;",
            "DELETE FROM credit_cards_mirror;",
            "DELETE FROM credit_cards_tombstones;",
            "DELETE FROM moz_meta;",
        ])
    }

    #[test]
    fn test_credit_card_engine_sync_finished() -> Result<()> {
        let db = new_mem_db();
        let credit_card_engine = create_engine(db);

        let last_sync = 24;
        let result =
            credit_card_engine.sync_finished(ServerTimestamp::from_millis(last_sync), Vec::new());
        assert!(result.is_ok());

        // check that last sync metadata was set
        let conn = &credit_card_engine.db.lock().unwrap().writer;

        assert_eq!(
            credit_card_engine.get_meta::<i64>(conn, LAST_SYNC_META_KEY)?,
            Some(last_sync)
        );

        Ok(())
    }

    #[test]
    fn test_credit_card_engine_get_sync_assoc() -> Result<()> {
        let db = new_mem_db();
        let credit_card_engine = create_engine(db);

        let result = credit_card_engine.get_sync_assoc();
        assert!(result.is_ok());

        // check that we disconnect if sync IDs not found
        assert_eq!(result.unwrap(), EngineSyncAssociation::Disconnected);

        // create sync metadata
        let global_guid = Guid::new("AAAA");
        let coll_guid = Guid::new("AAAA");
        let ids = CollSyncIds {
            global: global_guid,
            coll: coll_guid,
        };
        {
            let conn = &credit_card_engine.db.lock().unwrap().writer;
            credit_card_engine.put_meta(conn, GLOBAL_SYNCID_META_KEY, &ids.global)?;
            credit_card_engine.put_meta(conn, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
        }

        let result = credit_card_engine.get_sync_assoc();
        assert!(result.is_ok());

        // check that we return the metadata
        assert_eq!(result.unwrap(), EngineSyncAssociation::Connected(ids));
        Ok(())
    }

    #[test]
    fn test_engine_sync_reset() -> Result<()> {
        let db = new_mem_db();

        let tx = db.writer.unchecked_transaction()?;
        // create a normal record, a mirror record and a tombstone.
        let cc = InternalCreditCard {
            guid: Guid::random(),
            cc_name: "Ms Jane Doe".to_string(),
            cc_number: "1234".to_string(),
            cc_exp_month: 12,
            cc_exp_year: 2021,
            cc_type: "visa".to_string(),
            ..Default::default()
        };
        add_internal_credit_card(&tx, &cc)?;
        insert_mirror_record(&tx, &cc);
        insert_tombstone_record(&tx, Guid::random().to_string())?;
        tx.commit()?;

        let engine = create_engine(db);

        // create sync metadata
        let global_guid = Guid::new("AAAA");
        let coll_guid = Guid::new("AAAA");
        let ids = CollSyncIds {
            global: global_guid.clone(),
            coll: coll_guid.clone(),
        };
        {
            let conn = &engine.db.lock().unwrap().writer;
            engine.put_meta(conn, GLOBAL_SYNCID_META_KEY, &ids.global)?;
            engine.put_meta(conn, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
        }

        // call reset for sign out
        engine
            .reset(&EngineSyncAssociation::Disconnected)
            .expect("should work");

        // check that sync change counter has been reset
        {
            let conn = &engine.db.lock().unwrap().writer;
            let reset_record_exists: bool = conn.query_row(
                "SELECT EXISTS (
                    SELECT 1
                    FROM credit_cards_data
                    WHERE sync_change_counter = 1
                )",
                NO_PARAMS,
                |row| row.get(0),
            )?;
            assert!(reset_record_exists);

            // check that the mirror and tombstone tables have no records
            assert!(get_all(conn, "credit_cards_mirror".to_string())?.is_empty());
            assert!(get_all(conn, "credit_cards_tombstones".to_string())?.is_empty());

            // check that the last sync time was reset to 0
            let expected_sync_time = 0;
            assert_eq!(
                engine
                    .get_meta::<i64>(conn, LAST_SYNC_META_KEY)?
                    .unwrap_or(1),
                expected_sync_time
            );

            // check that the meta records were deleted
            assert!(engine
                .get_meta::<String>(conn, GLOBAL_SYNCID_META_KEY)?
                .is_none());
            assert!(engine
                .get_meta::<String>(conn, COLLECTION_SYNCID_META_KEY)?
                .is_none());

            clear_cc_tables(conn)?;

            // re-populating the tables
            let tx = conn.unchecked_transaction()?;
            add_internal_credit_card(&tx, &cc)?;
            insert_mirror_record(&tx, &cc);
            insert_tombstone_record(&tx, Guid::random().to_string())?;
            tx.commit()?;
        }

        // call reset for sign in
        engine
            .reset(&EngineSyncAssociation::Connected(ids))
            .expect("should work");

        let conn = &engine.db.lock().unwrap().writer;
        // check that the meta records were set
        let retrieved_global_sync_id = engine.get_meta::<String>(conn, GLOBAL_SYNCID_META_KEY)?;
        assert_eq!(
            retrieved_global_sync_id.unwrap_or_default(),
            global_guid.to_string()
        );

        let retrieved_coll_sync_id = engine.get_meta::<String>(conn, COLLECTION_SYNCID_META_KEY)?;
        assert_eq!(
            retrieved_coll_sync_id.unwrap_or_default(),
            coll_guid.to_string()
        );
        Ok(())
    }
}
