/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use service::Sync15Service;
use bso_record::{BsoRecord, CleartextRecord, Sync15Record, EncryptedPayload};
use request::{PostQueue, NormalResponseHandler};
use util::ServerTimestamp;
use tombstone::MaybeTombstone;
use error::{self, ErrorKind, Result};
use key_bundle::KeyBundle;
use serde_json as json;

#[derive(Debug, Clone)]
pub struct RecordChangeset<T> {
    pub changed: Vec<BsoRecord<T>>,
    pub deleted_ids: Vec<String>,
    /// For GETs, the last sync timestamp that should be persisted after
    /// applying the records.
    /// For POSTs, this is the XIUS timestamp.
    pub timestamp: ServerTimestamp,
    collection: String,
}

impl<T: Sync15Record> RecordChangeset<T> {
    pub fn new(timestamp: ServerTimestamp) -> RecordChangeset<T> {
        RecordChangeset {
            changed: vec![],
            deleted_ids: vec![],
            timestamp,
            collection: T::collection_tag().into()
        }
    }

    // TryFrom is unstable...
    pub fn from_json(json: RecordChangeset<json::Value>) -> Result<RecordChangeset<T>> {
        let RecordChangeset { changed, deleted_ids, timestamp, collection } = json;
        let mut result = RecordChangeset {
            changed: Vec::with_capacity(changed.len()),
            deleted_ids,
            timestamp,
            collection
        };
        for bso in changed.into_iter() {
            result.changed.push(
                bso.map_payload(|p| json::from_value(p))
                   .transpose()?);
        }
        Ok(result)
    }
}

// Not ideal, since it actually could be encrypted...
impl CleartextRecord for json::Value {}

// TODO: use a trait to unify this with the non-json versions
impl RecordChangeset<json::Value> {
    pub fn new_json(
        collection: String,
        timestamp: ServerTimestamp
    ) -> RecordChangeset<json::Value> {
        RecordChangeset {
            changed: vec![],
            deleted_ids: vec![],
            timestamp,
            collection,
        }
    }

    pub fn encrypt(self, key: &KeyBundle) -> error::Result<Vec<BsoRecord<EncryptedPayload>>> {
        let mut records = Vec::with_capacity(self.changed.len() + self.deleted_ids.len());
        let RecordChangeset { deleted_ids, changed, collection, .. } = self;
        for id in deleted_ids.into_iter() {
            let payload = json!({ id: id.clone(), deleted: true });
            let tombstone_bso: BsoRecord<json::Value> = BsoRecord {
                id,
                payload,
                collection: collection.clone(),
                modified: ServerTimestamp(0.0),
                sortindex: None,
                ttl: None,
            };
            records.push(tombstone_bso.encrypt(&key)?);
        }
        for bso in changes.into_iter() {
            records.push(bso.encrypt(&key)?);
        }
        Ok(records)
    }

    pub fn post(self, svc: &Sync15Service, fully_atomic: bool) -> Result<(Vec<String>, Vec<String>)> {
        Ok(CollectionUpdate::new(svc, self, fully_atomic)?.upload()?)
    }

    pub fn fetch(
        svc: &Sync15Service,
        collection: String,
        since: ServerTimestamp
    ) -> RecordChangeset<json::Value> {
        let records = svc.get_encrypted_records(&collection, since)?;
        let mut result = RecordChangeset::new_json(collection,
                                                   svc.last_server_timestamp.get());
        // Most records are probably not tombstones.
        result.changed.reserve(records.len());

        let key = svc.key_for_collection(&result.collection)?;
        for record in records {
            // TODO: if we see a HMAC error, may need to update crypto/keys?
            let decrypted = record.decrypt::<json::Value>(&key)?;
            if decrypted.payload["deleted"] == json::Value::Bool(true) {
                result.deleted_ids.push(decrypted.id);
            } else {
                result.changed.push(decrypted);
            }
        }
        Ok(result)
    }
}

impl<T: Sync15Record> From<RecordChangeset<T>> for RecordChangeset<json::Value> {
    fn from(records: RecordChangeset<T>) -> RecordChangeset<json::Value> {
        let RecordChangeset { changed, deleted_ids, timestamp, collection } = records;
        RecordChangeset {
            changed: changed.into_iter().map(|r|
                r.map_payload(|p|
                    json::to_value(p).expect(
                        "JSON.stringify equivalent failed, which should be impossible for us"))),
            deleted_ids,
            timestamp,
            collection
        }
    }
}

#[derive(Debug, Clone)]
struct CollectionUpdate<'a> {
    svc: &'a Sync15Service,
    collection: &'a str,
    xius: ServerTimestamp,
    to_update: Vec<BsoRecord<EncryptedPayload>>,
    fully_atomic: bool,
}

impl<'a> CollectionUpdate<'a> {
    pub fn new<T: CleartextRecord>(
        svc: &'a Sync15Service,
        changeset: RecordChangeset<T>,
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
        let mut q = self.svc.new_post_queue(self.collection,
                                            Some(self.xius),
                                            NormalResponseHandler::new(!self.fully_atomic))?;

        for record in self.to_update.into_iter() {
            let enqueued = q.enqueue(&encrypted)?;
            if !enqueued && self.fully_atomic {
                bail!(ErrorKind::RecordTooLargeError);
            }
        }

        q.flush(true)?;
        let (successful_ids, mut failed_ids) = q.successful_and_failed_ids();
        failed_ids.append(&mut failed);
        if !self.allow_dropped_records {
            assert_eq!(failed_ids.len(), 0,
                       "Bug: Should have failed by now if we aren't allowing dropped records");
        }
        Ok((successful_ids, failed_ids))
    }
}
