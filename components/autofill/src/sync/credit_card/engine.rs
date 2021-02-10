/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// use super::super::do_incoming;
// use super::incoming::{CreditCardsImpl, stage_incoming};
use crate::db::{
    credit_cards::reset_in_tx,
    store::{get_meta, put_meta},
    AutofillDb,
};
use interrupt_support::Interruptee;
use sync15::{
    telemetry, CollSyncIds, CollectionRequest, EngineSyncAssociation, IncomingChangeset,
    OutgoingChangeset, ServerTimestamp, SyncEngine,
};
use sync_guid::Guid as SyncGuid;

pub const LAST_SYNC_META_KEY: &str = "credit_cards_last_sync_time";
pub const GLOBAL_SYNCID_META_KEY: &str = "credit_cards_global_sync_id";
pub const COLLECTION_SYNCID_META_KEY: &str = "credit_cards_sync_id";

pub struct CreditCardEngine<'a> {
    pub db: &'a AutofillDb,
    #[allow(dead_code)]
    interruptee: &'a dyn Interruptee,
}

impl<'a> CreditCardEngine<'a> {}

impl<'a> SyncEngine for CreditCardEngine<'a> {
    #[inline]
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        "credit_cards".into()
    }

    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> anyhow::Result<OutgoingChangeset> {
        assert_eq!(inbound.len(), 1, "credit cards only requests one item");
        let inbound = inbound.into_iter().next().unwrap();

        // Stage all incoming items.
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let timestamp = inbound.timestamp;
        // TODO: Call yet-to-be-implemented incoming code
        // stage_incoming(&self.db.writer, inbound.changes.clone(), self.interruptee)?;
        // do_incoming(&self.db.writer, &CreditCardsImpl {}, self.interruptee)?;
        incoming_telemetry.applied(inbound.changes.len() as u32);
        telem.incoming(incoming_telemetry);

        // write the timestamp now, so if we are interrupted merging or
        // creating outgoing changesets we don't need to re-download the same
        // records.
        put_meta(self.db, LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;

        // Finally, stage outgoing items.
        // TODO: Call yet-to-be-implemented stage outgoing code
        //     let outgoing = self.fetch_outgoing_records(timestamp)?;
        //     Ok(outgoing)
        Ok(OutgoingChangeset::new(self.collection_name(), timestamp))
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        _records_synced: Vec<SyncGuid>,
    ) -> anyhow::Result<()> {
        put_meta(
            self.db,
            LAST_SYNC_META_KEY,
            &(new_timestamp.as_millis() as i64),
        )?;
        // TODO: Call yet-to-be implement stage outgoing code
        self.db.pragma_update(None, "wal_checkpoint", &"PASSIVE")?;
        Ok(())
    }

    fn get_collection_requests(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Vec<CollectionRequest>> {
        let since =
            ServerTimestamp(get_meta::<i64>(self.db, LAST_SYNC_META_KEY)?.unwrap_or_default());
        Ok(if since == server_timestamp {
            vec![]
        } else {
            vec![CollectionRequest::new(self.collection_name())
                .full()
                .newer_than(since)]
        })
    }

    fn get_sync_assoc(&self) -> anyhow::Result<EngineSyncAssociation> {
        let global = get_meta(self.db, GLOBAL_SYNCID_META_KEY)?;
        let coll = get_meta(self.db, COLLECTION_SYNCID_META_KEY)?;
        Ok(if let (Some(global), Some(coll)) = (global, coll) {
            EngineSyncAssociation::Connected(CollSyncIds { global, coll })
        } else {
            EngineSyncAssociation::Disconnected
        })
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> anyhow::Result<()> {
        let tx = self.db.unchecked_transaction()?;
        reset_in_tx(&tx, assoc)?;
        tx.commit()?;
        Ok(())
    }

    fn wipe(&self) -> anyhow::Result<()> {
        log::trace!(
            "credit cards doesn't implement wipe because there isn't a valid use case for it"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;
    use crate::error::*;

    #[test]
    fn test_credit_card_engine_sync_finished() -> Result<()> {
        let db = new_mem_db();
        let credit_card_engine = CreditCardEngine {
            db: &db,
            interruptee: &db.begin_interrupt_scope(),
        };

        let last_sync = 24;
        let result =
            credit_card_engine.sync_finished(ServerTimestamp::from_millis(last_sync), Vec::new());
        assert!(result.is_ok());

        // check that last sync metadata was set
        assert_eq!(
            get_meta::<i64>(&db, LAST_SYNC_META_KEY)?.unwrap_or_default(),
            last_sync
        );

        Ok(())
    }

    #[test]
    fn test_credit_card_engine_get_sync_assoc() -> Result<()> {
        let db = new_mem_db();
        let credit_card_engine = CreditCardEngine {
            db: &db,
            interruptee: &db.begin_interrupt_scope(),
        };

        let result = credit_card_engine.get_sync_assoc();
        assert!(result.is_ok());

        // check that we disconnect if sync IDs not found
        assert_eq!(result.unwrap(), EngineSyncAssociation::Disconnected);

        // create sync metadata
        let global_guid = SyncGuid::new("AAAA");
        let coll_guid = SyncGuid::new("AAAA");
        let ids = CollSyncIds {
            global: global_guid,
            coll: coll_guid,
        };
        put_meta(&db, GLOBAL_SYNCID_META_KEY, &ids.global)?;
        put_meta(&db, COLLECTION_SYNCID_META_KEY, &ids.coll)?;

        let result = credit_card_engine.get_sync_assoc();
        assert!(result.is_ok());

        // check that we return the metadata
        assert_eq!(result.unwrap(), EngineSyncAssociation::Connected(ids));
        Ok(())
    }
}
