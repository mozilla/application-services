/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use bso_record::{EncryptedBso, Payload};
use client::Sync15StorageClient;
use error::{self, ErrorKind, Result};
use key_bundle::KeyBundle;
use request::{NormalResponseHandler, UploadInfo, CollectionRequest};
use state::GlobalState;
use util::ServerTimestamp;

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
    pub fn new(
        collection: String,
        timestamp: ServerTimestamp
    ) -> RecordChangeset<T> {
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
        state: &GlobalState,
        fully_atomic: bool,
    ) -> Result<UploadInfo> {
        Ok(CollectionUpdate::new_from_changeset(client, state, self, fully_atomic)?.upload()?)
    }
}

impl IncomingChangeset {
    pub fn fetch(
        client: &Sync15StorageClient,
        state: &GlobalState,
        collection: String,
        collection_request: &CollectionRequest,
    ) -> Result<IncomingChangeset> {
        let records = client.get_encrypted_records(collection_request)?;
        let timestamp = state.last_modified_or_zero(&collection);
        let mut result = IncomingChangeset::new(collection, timestamp);
        result.changes.reserve(records.len());
        let key = state.key_for_collection(&result.collection)?;
        for record in records {
            // TODO: if we see a HMAC error, may need to update crypto/keys?
            let decrypted = record.decrypt(&key)?;
            result.changes.push(decrypted.into_timestamped_payload());
        }
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct CollectionUpdate<'a, 'b> {
    client: &'a Sync15StorageClient,
    state: &'b GlobalState,
    collection: String,
    xius: ServerTimestamp,
    to_update: Vec<EncryptedBso>,
    fully_atomic: bool,
}

impl<'a, 'b> CollectionUpdate<'a, 'b> {
    pub fn new(
        client: &'a Sync15StorageClient,
        state: &'b GlobalState,
        collection: String,
        xius: ServerTimestamp,
        records: Vec<EncryptedBso>,
        fully_atomic: bool,
    ) -> CollectionUpdate<'a, 'b> {
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
        state: &'b GlobalState,
        changeset: OutgoingChangeset,
        fully_atomic: bool,
    ) -> Result<CollectionUpdate<'a, 'b>> {
        let collection = changeset.collection.clone();
        let key_bundle = state.key_for_collection(&collection)?;
        let xius = changeset.timestamp;
        if xius < state.last_modified_or_zero(&collection) {
            // Not actually interrupted, but we know we'd fail the XIUS check.
            return Err(ErrorKind::BatchInterrupted.into());
        }
        let to_update = changeset.encrypt(&key_bundle)?;
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
            assert_eq!(info.failed_ids.len(), 0,
                       "Bug: Should have failed by now if we aren't allowing dropped records");
        }
        Ok(info)
    }
}
