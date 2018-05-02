/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */


use std::cell::Cell;
use std::time::{Duration};
use std::collections::{HashMap, HashSet};

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sync15ServiceInit {
    pub key_id: String,
    pub access_token: String,
    pub sync_key: String,
    pub tokenserver_base_url: String,
}

#[derive(Debug)]
pub struct Sync15Service {
    init_params: Sync15ServiceInit,
    root_key: KeyBundle,
    client: Client,
    // We update this when we make requests
    last_server_time: Cell<ServerTimestamp>,
    tsc: token::TokenserverClient,

    keys: Option<CollectionKeys>,
    server_config: Option<InfoConfiguration>,
    last_sync_remote: HashMap<String, ServerTimestamp>,
}

impl Sync15Service {
    pub fn new(init_params: Sync15ServiceInit) -> error::Result<Sync15Service> {
        let root_key = KeyBundle::from_ksync_base64(&init_params.sync_key)?;
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        // TODO Should we be doing this here? What if we get backoff? Who handles that?
        let tsc = token::TokenserverClient::new(&client,
                                                &init_params.tokenserver_base_url,
                                                init_params.access_token.clone(),
                                                init_params.key_id.clone())?;
        let timestamp = tsc.server_timestamp();
        Ok(Sync15Service {
            client,
            init_params,
            root_key,
            tsc,
            last_server_time: Cell::new(timestamp),
            keys: None,
            server_config: None,
            last_sync_remote: HashMap::new(),
        })
    }

    #[inline]
    fn authorized(&self, mut req: Request) -> error::Result<Request> {
        let header = self.tsc.authorization(&req)?;
        req.headers_mut().set(header);
        Ok(req)
    }

    // TODO: probably want a builder-like API to do collection requests (e.g. something
    // that occupies roughly the same conceptual role as the Collection class in desktop)
    fn build_request(&self, method: Method, url: Url) -> error::Result<Request> {
        self.authorized(self.client.request(method, url).header(Accept::json()).build()?)
    }

    fn relative_storage_request<T>(&self, method: Method, relative_path: T) -> error::Result<Response> where T: AsRef<str> {
        let s = self.tsc.token().api_endpoint.clone() + "/";
        let url = Url::parse(&s)?.join(relative_path.as_ref())?;
        Ok(self.make_storage_request(method, url)?)
    }

    fn make_storage_request(&self, method: Method, url: Url) -> error::Result<Response> {
        // I'm shocked that method isn't Copy...
        Ok(self.exec_request(self.build_request(method.clone(), url)?, true)?)
    }

    fn exec_request(&self, req: Request, require_success: bool) -> error::Result<Response> {
        let resp = self.client.execute(req)?;

        self.update_timestamp(resp.headers());

        if require_success && !resp.status().is_success() {
            error!("HTTP error {} ({}) during storage request to {}",
                   resp.status().as_u16(), resp.status(), resp.url().path());
            bail!(error::ErrorKind::StorageHttpError(
                resp.status(), resp.url().path().into()));
        }

        // TODO:
        // - handle backoff
        // - x-weave-quota?
        // - ... almost certainly other things too...

        Ok(resp)
    }

    fn collection_request(&self, method: Method, r: &CollectionRequest) -> error::Result<Response> {
        self.make_storage_request(method.clone(),
                                  r.build_url(Url::parse(&self.tsc.token().api_endpoint)?)?)
    }

    fn fetch_info<T>(&self, path: &str) -> error::Result<T> where for <'a> T: serde::de::Deserialize<'a> {
        let mut resp = self.relative_storage_request(Method::Get, path)?;
        let result: T = resp.json()?;
        Ok(result)
    }

    pub fn remote_setup(&mut self) -> error::Result<()> {
        let server_config = self.fetch_info::<InfoConfiguration>("info/configuration")?;
        self.server_config = Some(server_config);
        let mut resp = match self.relative_storage_request(Method::Get, "storage/meta/global") {
            Ok(r) => r,
            // This is gross, but at least it works. Replace 404s on meta/global with NoMetaGlobal.
            Err(error::Error(error::ErrorKind::StorageHttpError(StatusCode::NotFound, ..), _)) =>
                bail!(error::ErrorKind::NoMetaGlobal),
            Err(e) => return Err(e),
        };
        // Note: meta/global is not encrypted!
        let meta_global: BsoRecord<MetaGlobalRecord> = resp.json()?;
        info!("Meta global: {:?}", meta_global.payload);
        let collections = self.fetch_info::<HashMap<String, ServerTimestamp>>("info/collections")?;
        self.update_keys(&collections)?;
        self.last_sync_remote = collections;
        Ok(())
    }

    fn update_keys(&mut self, _info_collections: &HashMap<String, ServerTimestamp>) -> error::Result<()> {
        // TODO: if info/collections says we should, upload keys.
        // TODO: This should be handled in collection_keys.rs, which should track modified time, etc.
        let mut keys_resp = self.relative_storage_request(Method::Get, "storage/crypto/keys")?;
        let keys: BsoRecord<EncryptedPayload> = keys_resp.json()?;
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
        let mut resp = self.collection_request(Method::Get, CollectionRequest::new(collection).full())?;
        let records: Vec<BsoRecord<EncryptedPayload>> = resp.json()?;
        records.into_iter()
               .map(|record| record.decrypt::<MaybeTombstone<T>>(key))
               .collect()
    }

    fn update_timestamp(&self, hs: &header::Headers) {
        if let Some(ts) = hs.get::<XWeaveTimestamp>().map(|h| **h) {
            self.last_server_time.set(ts);
        } else {
            // Should we complain more here?
            warn!("No X-Weave-Timestamp from storage server!");
        }
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
        let pw = PostWrapper { svc: self, coll: coll.into() };
        Ok(PostQueue::new(self.server_config.as_ref().unwrap(), ts, pw, on_response))
    }
}

struct PostWrapper<'a> {
    svc: &'a Sync15Service,
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
        let url = CollectionRequest::new(self.coll.clone())
                                    .batch(batch)
                                    .commit(commit)
                                    .build_url(Url::parse(&self.svc.tsc.token().api_endpoint)?)?;

        let mut req = self.svc.build_request(Method::Post, url)?;
        req.headers_mut().set(header::ContentType::json());
        req.headers_mut().set(XIfUnmodifiedSince(xius));
        // It's very annoying that we need to copy the body here, the request
        // shouldn't need to take ownership of it...
        *req.body_mut() = Some(Vec::from(bytes).into());
        let mut resp = self.svc.exec_request(req, false)?;
        Ok(PostResponse::from_response(&mut resp)?)
    }
}

#[derive(Debug, Clone)]
pub struct CollectionUpdate<'a, T> {
    svc: &'a Sync15Service,
    last_sync: ServerTimestamp,
    to_update: Vec<MaybeTombstone<T>>,
    allow_dropped_records: bool,
    queued_ids: HashSet<String>
}

impl<'a, T> CollectionUpdate<'a, T> where T: Sync15Record {
    pub fn new(svc: &'a Sync15Service, allow_dropped_records: bool) -> CollectionUpdate<'a, T> {
        let coll = T::collection_tag();
        let ts = svc.last_modified_or_zero(coll);
        CollectionUpdate {
            svc,
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
        let key = self.svc.key_for_collection(T::collection_tag())?;
        let mut q = self.svc.new_post_queue(T::collection_tag(), Some(self.last_sync),
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
