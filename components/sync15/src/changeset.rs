/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::bso_record::{EncryptedBso, Payload};
use crate::client::{Sync15ClientResponse, Sync15StorageClient};
use crate::error::{self, ErrorKind, Result};
use crate::key_bundle::KeyBundle;
use crate::request::{CollectionRequest, NormalResponseHandler, UploadInfo};
use crate::util::ServerTimestamp;
use crate::CollState;

#[derive(Debug, Clone)]
pub struct RecordChangeset<Payload> {
    pub changes: Vec<Payload>,
    /// For GETs, the last sync timestamp that should be persisted after
    /// applying the records.
    /// For POSTs, this is the XIUS timestamp.
    pub timestamp: ServerTimestamp,
    pub collection: String,
}

pub type IncomingChangeset = RecordChangeset<(Payload, ServerTimestamp)>;
pub type OutgoingChangeset = RecordChangeset<Payload>;

// TODO: use a trait to unify this with the non-json versions
impl<T> RecordChangeset<T> {
    #[inline]
    pub fn new(collection: String, timestamp: ServerTimestamp) -> RecordChangeset<T> {
        RecordChangeset {
            changes: vec![],
            timestamp,
            collection,
        }
    }
}

impl OutgoingChangeset {
    pub fn encrypt(self, key: &KeyBundle) -> Result<Vec<EncryptedBso>> {
        let RecordChangeset {
            changes,
            collection,
            ..
        } = self;
        changes
            .into_iter()
            .map(|change| change.into_bso(collection.clone()).encrypt(key))
            .collect()
    }

    pub fn post(
        self,
        client: &Sync15StorageClient,
        state: &CollState,
        fully_atomic: bool,
    ) -> Result<UploadInfo> {
        Ok(CollectionUpdate::new_from_changeset(client, state, self, fully_atomic)?.upload()?)
    }
}

impl IncomingChangeset {
    pub fn fetch(
        client: &Sync15StorageClient,
        state: &mut CollState,
        collection: String,
        collection_request: &CollectionRequest,
    ) -> Result<IncomingChangeset> {
        let (records, timestamp) = match client.get_encrypted_records(collection_request)? {
            Sync15ClientResponse::Success {
                record,
                last_modified,
                ..
            } => (record, last_modified),
            other => return Err(other.create_storage_error().into()),
        };
        // xxx - duplication below of `timestamp` smells wrong
        state.last_modified = timestamp;
        let mut result = IncomingChangeset::new(collection, timestamp);
        result.changes.reserve(records.len());
        for record in records {
            // if we see a HMAC error, we've made an explicit decision to
            // NOT handle it here, but restart the global state machine.
            // That should cause us to re-read crypto/keys and things should
            // work (although if for some reason crypto/keys was updated but
            // not all storage was wiped we are probably screwed.)
            let decrypted = record.decrypt(&state.key)?;
            result.changes.push(decrypted.into_timestamped_payload());
        }
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct CollectionUpdate<'a> {
    client: &'a Sync15StorageClient,
    state: &'a CollState,
    collection: String,
    xius: ServerTimestamp,
    to_update: Vec<EncryptedBso>,
    fully_atomic: bool,
}

impl<'a> CollectionUpdate<'a> {
    pub fn new(
        client: &'a Sync15StorageClient,
        state: &'a CollState,
        collection: String,
        xius: ServerTimestamp,
        records: Vec<EncryptedBso>,
        fully_atomic: bool,
    ) -> CollectionUpdate<'a> {
        CollectionUpdate {
            client,
            state,
            collection,
            xius,
            to_update: records,
            fully_atomic,
        }
    }

    pub fn new_from_changeset(
        client: &'a Sync15StorageClient,
        state: &'a CollState,
        changeset: OutgoingChangeset,
        fully_atomic: bool,
    ) -> Result<CollectionUpdate<'a>> {
        let collection = changeset.collection.clone();
        let xius = changeset.timestamp;
        if xius < state.last_modified {
            // Not actually interrupted, but we know we'd fail the XIUS check.
            return Err(ErrorKind::BatchInterrupted.into());
        }
        let to_update = changeset.encrypt(&state.key)?;
        Ok(CollectionUpdate::new(
            client,
            state,
            collection,
            xius,
            to_update,
            fully_atomic,
        ))
    }

    /// Returns a list of the IDs that failed if allowed_dropped_records is true, otherwise
    /// returns an empty vec.
    pub fn upload(self) -> error::Result<UploadInfo> {
        let mut failed = vec![];
        let mut q = self.client.new_post_queue(
            &self.collection,
            &self.state.config,
            self.xius,
            NormalResponseHandler::new(!self.fully_atomic),
        )?;

        for record in self.to_update.into_iter() {
            let enqueued = q.enqueue(&record)?;
            if !enqueued && self.fully_atomic {
                return Err(ErrorKind::RecordTooLargeError.into());
            }
        }

        q.flush(true)?;
        let mut info = q.completed_upload_info();
        info.failed_ids.append(&mut failed);
        if self.fully_atomic {
            assert_eq!(
                info.failed_ids.len(),
                0,
                "Bug: Should have failed by now if we aren't allowing dropped records"
            );
        }
        Ok(info)
    }
}
