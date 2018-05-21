/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */


use std::cell::Cell;
use std::time::{Duration};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use reqwest::{
    Client,
    Request,
    Response,
    Url,
    header::{self, Accept}
};
use hyper::{Method, StatusCode};
use serde;

use util::{ServerTimestamp, SERVER_EPOCH};
use token;
use error;
use key_bundle::KeyBundle;
use bso_record::{BsoRecord, Sync15Record, EncryptedPayload};
use tombstone::{MaybeTombstone, NonTombstone};
use record_types::MetaGlobalRecord;
use collection_keys::CollectionKeys;
use request::{
    CollectionRequest,
    InfoConfiguration,
    XWeaveTimestamp,
    XIfUnmodifiedSince,
    PostResponse,
    BatchPoster,
    PostQueue,
    PostResponseHandler,
    NormalResponseHandler,
};
use storage_client::StorageClient;

#[derive(Debug)]
pub struct Collections {
    root_key: KeyBundle,
    keys: Option<CollectionKeys>,
    storage_client: Rc<StorageClient>,

    server_config: Option<InfoConfiguration>,
    last_sync_remote: HashMap<String, ServerTimestamp>,
}

impl Collections {
    pub fn new(storage_client: Rc<StorageClient>, sync_key: &str) -> error::Result<Collections> {
        let root_key = KeyBundle::from_ksync_base64(sync_key)?;
        Ok(Collections {
            root_key,
            keys: None,
            storage_client,
            server_config: None,
            last_sync_remote: HashMap::new(),
        })
    }

    // If we're not logged in, run remote setup, where we fetch `info/collections` and stash keys.
    // If 200, do _remoteSetup, where we verify and update keys.
    // Otherwise, if we are logged in, fetch `info/collections` so we can get last modified times for each collection.
    // Still want to run `_remoteSetup` for the logged in case, too!
    // Fetch `meta/global`. (Our record layer caches this, so no request in most cases?)
    // If `meta/global` has newer modified date, clobber the cached record, fetch anew, and handle auth/missing errors.
    // Compare storage versions and sync IDs (the *global* sync ID, not per-collection)
    //
    // On sync ID mismatch, fetch remote keys and try to verify them. (If we don't have an up-to-date
    // kSync, HMAC verification will fail, and we'll start over).
    //
    // If we don't have a `meta/global`, no sync ID, or our local storage version is newer
    // than the server's, wipe the server, upload a new meta/global and start over.
    // We won't be doing this in Lockbox.

    // Done after login, but not necessarily before every sync.
    // (As an optimization, we can use the old keys. If the key changes underneath us, we re-fetch).
    // Should we fetch all keys, or just the ones for collections that we know about?
    // (Probably all; if we need to change the key, we shouldn't clobber other keys that are already there)
    // Desktop implementation saves all keys, even when it can't sync the collection. And we shouldn't be
    // regenerating keys for those, right?
    //
    // `info/collections` gives us modified times.
    pub fn remote_setup(&mut self) -> error::Result<()> {
        let server_config = self.storage_client.fetch_info_configuration()?;
        self.server_config = Some(server_config);
        let meta_global = self.storage_client.fetch_meta_global()?;
        info!("Meta global: {:?}", meta_global.payload);
        let collections = self.storage_client.fetch_info_collections()?;
        self.update_keys(&collections)?;
        self.last_sync_remote = collections;
        Ok(())
    }

    fn update_keys(&mut self, _info_collections: &HashMap<String, ServerTimestamp>) -> error::Result<()> {
        let keys = self.storage_client.fetch_crypto_keys()?;
        self.keys = Some(CollectionKeys::from_encrypted_bso(keys, &self.root_key)?);
        // TODO: error handling... key upload?
        Ok(())
    }

    pub fn key_for_collection(&self, collection: &str) -> error::Result<&KeyBundle> {
        Ok(self.keys.as_ref()
                    .ok_or_else(|| error::unexpected("Don't have keys (yet?)"))?
                    .key_for_collection(collection))
    }

    pub fn all_records<T>(&mut self, collection: &str) ->
            error::Result<Vec<BsoRecord<MaybeTombstone<T>>>> where T: Sync15Record {
        let key = self.key_for_collection(collection)?;
        let records = self.storage_client.fetch_full_collection(collection)?;
        records.into_iter()
               .map(|record| record.decrypt::<MaybeTombstone<T>>(key))
               .collect()
    }

    pub fn last_modified(&self, coll: &str) -> Option<ServerTimestamp> {
        self.last_sync_remote.get(coll).cloned()
    }

    pub fn last_modified_or_zero(&self, coll: &str) -> ServerTimestamp {
        self.last_modified(coll).unwrap_or(SERVER_EPOCH)
    }

    fn new_post_queue<'a, F: PostResponseHandler>(&'a self, coll: &str, lm: Option<ServerTimestamp>, on_response: F)
            -> error::Result<PostQueue<PostWrapper<'a>, F>> {
        let ts = lm.unwrap_or_else(|| self.last_modified_or_zero(&coll));
        let pw = PostWrapper { collections: self, coll: coll.into() };
        Ok(PostQueue::new(self.server_config.as_ref().unwrap(), ts, pw, on_response))
    }
}

struct PostWrapper<'a> {
    collections: &'a Collections,
    coll: String,
}

impl<'a> BatchPoster for PostWrapper<'a> {
    fn post<T, O>(&self,
                  bytes: &[u8],
                  xius: ServerTimestamp,
                  batch: Option<String>,
                  commit: bool,
                  _: &PostQueue<T, O>) -> error::Result<PostResponse>
    {
        self.collections.storage_client.post::<T, O>(&self.coll, bytes, xius, batch, commit)
    }
}

#[derive(Debug, Clone)]
pub struct CollectionUpdate<'a, T> {
    collections: &'a Collections,
    last_sync: ServerTimestamp,
    to_update: Vec<MaybeTombstone<T>>,
    allow_dropped_records: bool,
    queued_ids: HashSet<String>
}

impl<'a, T> CollectionUpdate<'a, T> where T: Sync15Record {
    pub fn new(collections: &'a Collections, allow_dropped_records: bool) -> CollectionUpdate<'a, T> {
        let coll = T::collection_tag();
        let ts = collections.last_modified_or_zero(coll);
        CollectionUpdate {
            collections,
            last_sync: ts,
            to_update: vec![],
            allow_dropped_records,
            queued_ids: HashSet::new(),
        }
    }

    pub fn add(&mut self, rec_or_tombstone: MaybeTombstone<T>) {
        // Block to limit scope of the `id` borrow.
        {
            let id = rec_or_tombstone.record_id();
            // Should this be an Err and not an assertion?
            assert!(!self.queued_ids.contains(id),
                    "Attempt to update ID multiple times in the same batch {}", id);
            self.queued_ids.insert(id.into());
        }
        self.to_update.push(rec_or_tombstone);
    }

    pub fn add_record(&mut self, record: T) {
        self.add(NonTombstone(record));
    }

    pub fn add_tombstone(&mut self, id: String) {
        self.add(MaybeTombstone::tombstone(id));
    }

    /// Returns a list of the IDs that failed if allowed_dropped_records is true, otherwise
    /// returns an empty vec.
    pub fn upload(self) -> error::Result<(Vec<String>, Vec<String>)> {
        let mut failed = vec![];
        let key = self.collections.key_for_collection(T::collection_tag())?;
        let mut q = self.collections.new_post_queue(T::collection_tag(), Some(self.last_sync),
            NormalResponseHandler::new(self.allow_dropped_records))?;

        for record in self.to_update.into_iter() {
            let record_cleartext: BsoRecord<MaybeTombstone<T>> = record.into();
            let encrypted = record_cleartext.encrypt(key)?;
            let enqueued = q.enqueue(&encrypted)?;
            if !enqueued && !self.allow_dropped_records {
                bail!(error::ErrorKind::RecordTooLargeError);
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
