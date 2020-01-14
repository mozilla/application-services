/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::RemergeSync;
use crate::error::*;
use sync15_traits::*;

pub struct RemergeStore<'a>(std::cell::RefCell<RemergeSync<'a>>);

impl<'a> Store for RemergeStore<'a> {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        self.0.borrow().db.collection().to_owned().into()
    }
    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> Result<OutgoingChangeset, failure::Error> {
        Ok(self.0.borrow_mut().apply_incoming(inbound, telem)?)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: Vec<Guid>,
    ) -> Result<(), failure::Error> {
        Ok(self
            .0
            .borrow_mut()
            .sync_finished(new_timestamp, records_synced)?)
    }

    fn get_collection_requests(&self) -> Result<Vec<CollectionRequest>, failure::Error> {
        Ok(self.0.borrow_mut().get_collection_requests()?)
    }

    fn get_sync_assoc(&self) -> Result<StoreSyncAssociation, failure::Error> {
        unimplemented!("TODO");
    }

    fn reset(&self, _assoc: &StoreSyncAssociation) -> Result<(), failure::Error> {
        unimplemented!("TODO");
        // Ok(())
    }

    fn wipe(&self) -> Result<(), failure::Error> {
        unimplemented!("TODO");
        // Ok(())
    }
}
