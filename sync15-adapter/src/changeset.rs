/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use service::Sync15Service;
use bso_record::{Payload, EncryptedBso};
use request::{NormalResponseHandler, UploadInfo};
use util::ServerTimestamp;
use error::{self, ErrorKind, Result};
use key_bundle::KeyBundle;

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
        let RecordChangeset { changes, collection, .. } = self;
        changes.into_iter()
               .map(|change| change.into_bso(collection.clone()).encrypt(key))
               .collect()
    }

    pub fn post(self, svc: &Sync15Service, fully_atomic: bool) -> Result<UploadInfo> {
        Ok(CollectionUpdate::new_from_changeset(svc, self, fully_atomic)?.upload()?)
    }
}

impl IncomingChangeset {
    pub fn fetch(
        svc: &Sync15Service,
        collection: String,
        since: ServerTimestamp
    ) -> Result<IncomingChangeset> {
        let records = svc.get_encrypted_records(&collection, since)?;
        let mut result = IncomingChangeset::new(collection, svc.last_server_time());
        result.changes.reserve(records.len());
        let key = svc.key_for_collection(&result.collection)?;
        for record in records {
            // TODO: if we see a HMAC error, may need to update crypto/keys?
            let decrypted = record.decrypt(&key)?;
            result.changes.push(decrypted.into_timestamped_payload());
        }
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct CollectionUpdate<'a> {
    svc: &'a Sync15Service,
    collection: String,
    xius: ServerTimestamp,
    to_update: Vec<EncryptedBso>,
    fully_atomic: bool,
}

impl<'a> CollectionUpdate<'a> {

    pub fn new(svc: &'a Sync15Service,
               collection: String,
               xius: ServerTimestamp,
               records: Vec<EncryptedBso>,
               fully_atomic: bool) -> CollectionUpdate<'a> {
        CollectionUpdate {
            svc,
            collection,
            xius,
            to_update: records,
            fully_atomic,
        }
    }

    pub fn new_from_changeset(
        svc: &'a Sync15Service,
        changeset: OutgoingChangeset,
        fully_atomic: bool
    ) -> Result<CollectionUpdate<'a>> {
        let collection = changeset.collection.clone();
        let key_bundle = svc.key_for_collection(&collection)?;
        let xius = changeset.timestamp;
        if xius < svc.last_modified_or_zero(&collection) {
            // Not actually interrupted, but we know we'd fail the XIUS check.
            return Err(ErrorKind::BatchInterrupted.into());
        }
        let to_update = changeset.encrypt(&key_bundle)?;
        Ok(CollectionUpdate::new(svc, collection, xius, to_update, fully_atomic))
    }

    /// Returns a list of the IDs that failed if allowed_dropped_records is true, otherwise
    /// returns an empty vec.
    pub fn upload(self) -> error::Result<UploadInfo> {
        let mut failed = vec![];
        let mut q = self.svc.new_post_queue(&self.collection,
                                            Some(self.xius),
                                            NormalResponseHandler::new(!self.fully_atomic))?;

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
