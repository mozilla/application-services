/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// `error_chain!` can recurse deeply and I guess we're just supposed to live with that...
#![recursion_limit = "1024"]

extern crate serde;
extern crate base64;
extern crate openssl;
extern crate reqwest;
extern crate hawk;
#[macro_use]
extern crate hyper;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

extern crate serde_json;

#[macro_use]
extern crate error_chain;

extern crate url;

// TODO: Some of these don't need to be pub...
pub mod key_bundle;
pub mod error;
pub mod bso_record;
pub mod record_types;
pub mod token;
pub mod collection_keys;
pub mod util;
pub mod request;

pub use MaybeTombstone::*;

use util::ServerTimestamp;

use std::cell::Cell;
use std::time::{Duration};
use std::collections::HashMap;

use key_bundle::KeyBundle;
use reqwest::{
    Client,
    Request,
    Response,
    Url,
    header::{self, Accept}
};
use hyper::Method;
use bso_record::{BsoRecord, Sync15Record, EncryptedPayload};
use record_types::{MaybeTombstone, MetaGlobalRecord};
use collection_keys::CollectionKeys;
use request::{
    CollectionRequest,
    InfoConfiguration,
    XWeaveTimestamp,
    XIfUnmodifiedSince,
    PostResponse,
    BatchPoster,
    PostQueue
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
            Err(error::Error(error::ErrorKind::StorageHttpError(hyper::StatusCode::NotFound, ..), _)) =>
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

    pub fn all_records<T>(&mut self, collection: &str) -> error::Result<Vec<BsoRecord<T>>> where T: Sync15Record {
        let key = self.key_for_collection(collection)?;
        let mut resp = self.collection_request(Method::Get, CollectionRequest::new(collection).full())?;
        let records: Vec<BsoRecord<EncryptedPayload>> = resp.json()?;
        let mut result = Vec::with_capacity(records.len());
        for record in records {
            let decrypted: BsoRecord<MaybeTombstone<T>> = record.decrypt(key)?;
            if let Some(record) = decrypted.record() {
                result.push(record);
            }
        }
        Ok(result)
    }

    fn update_timestamp(&self, hs: &header::Headers) {
        if let Some(ts) = hs.get::<XWeaveTimestamp>().map(|h| **h) {
            self.last_server_time.set(ts);
        } else {
            // Should we complain more here?
            warn!("No X-Weave-Timestamp from storage server!");
        }
    }

    fn new_post_queue<'a, F: FnMut(PostResponse, bool) -> error::Result<()>>(&'a self, coll: &str, on_response: F)
            -> error::Result<PostQueue<PostWrapper<'a>, F>> {
        let ts = self.last_sync_remote
                     .get(coll)
                     .ok_or_else(|| error::unexpected(format!("Unknown collection {}", coll)))?;
        let pw = PostWrapper { svc: self, coll: coll.into() };
        Ok(PostQueue::new(self.server_config.as_ref().unwrap(), *ts, pw, on_response))
    }
}

struct PostWrapper<'a> {
    svc: &'a Sync15Service,
    coll: String,
}

impl<'a> BatchPoster for PostWrapper<'a> {
    fn post(&mut self, bytes: &[u8], xius: ServerTimestamp, batch: Option<String>, commit: bool) -> error::Result<PostResponse> {
        let url = CollectionRequest::new(self.coll.clone())
                                    .batch(batch)
                                    .commit(commit)
                                    .build_url(Url::parse(&self.svc.tsc.token().api_endpoint)?)?;

        let mut req = self.svc.build_request(Method::Post, url)?;
        req.headers_mut().set(XIfUnmodifiedSince(xius));
        let mut resp = self.svc.exec_request(req, false)?;
        Ok(PostResponse::from_response(&mut resp)?)
    }
}
