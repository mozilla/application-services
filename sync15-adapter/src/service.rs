/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */


use std::cell::Cell;
use std::time::{Duration};
use std::collections::{HashMap};

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
use error::{self, ErrorKind};
use key_bundle::KeyBundle;
use bso_record::{BsoRecord, EncryptedBso};
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
    tsc: token::TokenProvider,

    keys: Option<CollectionKeys>,
    server_config: Option<InfoConfiguration>,
    last_sync_remote: HashMap<String, ServerTimestamp>,
}

impl Sync15Service {
    pub fn new(init_params: Sync15ServiceInit) -> error::Result<Sync15Service> {
        let root_key = KeyBundle::from_ksync_base64(&init_params.sync_key)?;
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        let tsc = token::TokenProvider::new(init_params.tokenserver_base_url.clone(),
                                            init_params.access_token.clone(),
                                            init_params.key_id.clone());
        let timestamp = ServerTimestamp(0f64);
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
        let header = self.tsc.authorization(&self.client, &req)?;
        req.headers_mut().set(header);
        Ok(req)
    }

    // TODO: probably want a builder-like API to do collection requests (e.g. something
    // that occupies roughly the same conceptual role as the Collection class in desktop)
    fn build_request(&self, method: Method, url: Url) -> error::Result<Request> {
        self.authorized(self.client.request(method, url).header(Accept::json()).build()?)
    }

    fn relative_storage_request<T>(&self, method: Method, relative_path: T) -> error::Result<Response> where T: AsRef<str> {
        let s = self.tsc.api_endpoint(&self.client)? + "/";
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
            return Err(ErrorKind::StorageHttpError {
                code: resp.status(),
                route: resp.url().path().into()
            }.into());
        }

        // TODO:
        // - handle backoff
        // - x-weave-quota?
        // - ... almost certainly other things too...

        Ok(resp)
    }

    fn collection_request(&self, method: Method, r: &CollectionRequest) -> error::Result<Response> {
        self.make_storage_request(method.clone(),
                                  r.build_url(Url::parse(&self.tsc.api_endpoint(&self.client)?)?)?)
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
            Err(e) => {
                if let ErrorKind::StorageHttpError { code: StatusCode::NotFound, .. } = e.kind() {
                    return Err(ErrorKind::NoMetaGlobal.into())
                }
                return Err(e)
            }
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
        let keys: EncryptedBso = keys_resp.json()?;
        self.keys = Some(CollectionKeys::from_encrypted_bso(keys, &self.root_key)?);
        // TODO: error handling... key upload?
        Ok(())
    }

    pub fn key_for_collection(&self, collection: &str) -> error::Result<&KeyBundle> {
        Ok(self.keys.as_ref()
                    .ok_or_else(|| ErrorKind::NoCryptoKeys)?
                    .key_for_collection(collection))
    }

    fn update_timestamp(&self, hs: &header::Headers) {
        if let Some(ts) = hs.get::<XWeaveTimestamp>().map(|h| **h) {
            self.last_server_time.set(ts);
        } else {
            // Should we complain more here?
            warn!("No X-Weave-Timestamp from storage server!");
        }
    }

    pub fn get_encrypted_records(
        &self,
        collection: &str,
        since: ServerTimestamp,
    ) -> error::Result<Vec<EncryptedBso>> {
        self.key_for_collection(collection)?;
        let mut resp = self.collection_request(Method::Get,
                                               CollectionRequest::new(collection)
                                                   .full()
                                                   .newer_than(since))?;
        Ok(resp.json()?)
    }

    pub fn last_modified(&self, coll: &str) -> Option<ServerTimestamp> {
        self.last_sync_remote.get(coll).cloned()
    }

    pub fn last_modified_or_zero(&self, coll: &str) -> ServerTimestamp {
        self.last_modified(coll).unwrap_or(SERVER_EPOCH)
    }

    pub fn new_post_queue<'a, F: PostResponseHandler>(&'a self, coll: &str, lm: Option<ServerTimestamp>, on_response: F)
            -> error::Result<PostQueue<PostWrapper<'a>, F>> {
        let ts = lm.unwrap_or_else(|| self.last_modified_or_zero(&coll));
        let pw = PostWrapper { svc: self, coll: coll.into() };
        Ok(PostQueue::new(self.server_config.as_ref().unwrap(), ts, pw, on_response))
    }

    #[inline]
    pub fn last_server_time(&self) -> ServerTimestamp {
        self.last_server_time.get()
    }
}

pub struct PostWrapper<'a> {
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
                                    .build_url(Url::parse(&self.svc.tsc.api_endpoint(&self.svc.client)?)?)?;

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
