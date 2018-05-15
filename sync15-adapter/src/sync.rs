/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use service::Sync15Service;
use bso_record::{BsoRecord, Cleartext, EncryptedBso};
use request::NormalResponseHandler;
use util::ServerTimestamp;
use error::{self, ErrorKind, Result};
use key_bundle::KeyBundle;

#[derive(Debug, Clone)]
pub struct RecordChangeset {
    pub changed: Vec<BsoRecord<Cleartext>>,
    pub deleted_ids: Vec<String>,
    /// For GETs, the last sync timestamp that should be persisted after
    /// applying the records.
    /// For POSTs, this is the XIUS timestamp.
    pub timestamp: ServerTimestamp,
    pub collection: String,
}

// TODO: use a trait to unify this with the non-json versions
impl RecordChangeset {
    pub fn new(
        collection: String,
        timestamp: ServerTimestamp
    ) -> RecordChangeset {
        RecordChangeset {
            changed: vec![],
            deleted_ids: vec![],
            timestamp,
            collection,
        }
    }

    pub fn encrypt(self, key: &KeyBundle) -> Result<Vec<EncryptedBso>> {
        let mut records = Vec::with_capacity(self.changed.len() + self.deleted_ids.len());
        let RecordChangeset { deleted_ids, changed, collection, .. } = self;

        for id in deleted_ids.into_iter() {
            let tombstone_bso = Cleartext::new_tombstone(id.clone())
                .into_bso(collection.clone(), None);
            records.push(tombstone_bso.encrypt(&key)?);
        }

        for bso in changed.into_iter() {
            // Should we should consumers pass in `ttl` or `sortindex`?
            records.push(bso.encrypt(&key)?);
        }
        Ok(records)
    }

    pub fn post(self, svc: &Sync15Service, fully_atomic: bool)
        -> Result<(Vec<String>, Vec<String>)>
    {
        Ok(CollectionUpdate::new(svc, self, fully_atomic)?.upload()?)
    }

    pub fn fetch(
        svc: &Sync15Service,
        collection: String,
        since: ServerTimestamp
    ) -> Result<RecordChangeset> {
        let records = svc.get_encrypted_records(&collection, since)?;
        let mut result = RecordChangeset::new(collection, svc.last_server_time());
        // Most records are probably not tombstones.
        result.changed.reserve(records.len());

        let key = svc.key_for_collection(&result.collection)?;
        for record in records {
            // TODO: if we see a HMAC error, may need to update crypto/keys?
            let decrypted = record.decrypt(&key)?;
            if decrypted.is_tombstone() {
                result.deleted_ids.push(decrypted.id);
            } else {
                result.changed.push(decrypted);
            }
        }
        Ok(result)
    }
}

#[derive(Debug, Clone)]
struct CollectionUpdate<'a> {
    svc: &'a Sync15Service,
    collection: String,
    xius: ServerTimestamp,
    to_update: Vec<EncryptedBso>,
    fully_atomic: bool,
}

impl<'a> CollectionUpdate<'a> {
    pub fn new(
        svc: &'a Sync15Service,
        changeset: RecordChangeset,
        fully_atomic: bool
    ) -> Result<CollectionUpdate<'a>> {
        let collection = changeset.collection.clone();
        let key_bundle = svc.key_for_collection(&collection)?;
        let xius = changeset.timestamp;
        if xius < svc.last_modified_or_zero(&collection) {
            // Not actually interrupted, but we know we'd fail the XIUS check.
            bail!(ErrorKind::BatchInterrupted);
        }
        let to_update = changeset.encrypt(&key_bundle)?;
        Ok(CollectionUpdate {
            svc,
            collection,
            xius,
            to_update,
            fully_atomic,
        })
    }

    /// Returns a list of the IDs that failed if allowed_dropped_records is true, otherwise
    /// returns an empty vec.
    pub fn upload(self) -> error::Result<(Vec<String>, Vec<String>)> {
        let mut failed = vec![];
        let mut q = self.svc.new_post_queue(&self.collection,
                                            Some(self.xius),
                                            NormalResponseHandler::new(!self.fully_atomic))?;

        for record in self.to_update.into_iter() {
            let enqueued = q.enqueue(&record)?;
            if !enqueued && self.fully_atomic {
                bail!(ErrorKind::RecordTooLargeError);
            }
        }

        q.flush(true)?;
        let (successful_ids, mut failed_ids) = q.successful_and_failed_ids();
        failed_ids.append(&mut failed);
        if self.fully_atomic {
            assert_eq!(failed_ids.len(), 0,
                       "Bug: Should have failed by now if we aren't allowing dropped records");
        }
        Ok((successful_ids, failed_ids))
    }
}
