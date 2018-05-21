/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::time::{Duration};
use std::collections::HashMap;
use std::rc::Rc;

use error;

use reqwest::{
    Client,
    Request,
    Response,
    Url,
    header::{self, Accept}
};
use hyper::{Method, StatusCode};
use serde;

use util::ServerTimestamp;
use token;
use bso_record::{BsoRecord, Sync15Record, EncryptedPayload};
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
pub struct StorageClientInit {
    pub key_id: String,
    pub access_token: String,
    pub tokenserver_base_url: String,
}

#[derive(Debug)]
pub struct StorageClient {
    client: Rc<Client>,
    tsc: token::TokenserverClient,
    // We update this when we make requests
    last_server_time: Cell<ServerTimestamp>,
}

impl StorageClient {
    pub fn new(init_params: StorageClientInit) -> error::Result<StorageClient> {
        let client = Rc::new(Client::builder().timeout(Duration::from_secs(30)).build()?);
        let tsc = token::TokenserverClient::new(client.clone(),
                                                init_params.tokenserver_base_url.clone(),
                                                init_params.access_token.clone(),
                                                init_params.key_id.clone());
        let timestamp = ServerTimestamp(0f64);
        Ok(StorageClient {
            client,
            tsc,
            last_server_time: Cell::new(timestamp),
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
        let s = self.tsc.api_endpoint()? + "/";
        let url = Url::parse(&s)?.join(relative_path.as_ref())?;
        Ok(self.make_storage_request(method, url)?)
    }

    fn make_storage_request(&self, method: Method, url: Url) -> error::Result<Response> {
        // I'm shocked that method isn't Copy...
        Ok(self.exec_request(self.build_request(method, url)?, true)?)
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
        self.make_storage_request(method,
                                  r.build_url(Url::parse(&self.tsc.api_endpoint()?)?)?)
    }

    fn fetch_info<T>(&self, path: &str) -> error::Result<T> where for <'a> T: serde::de::Deserialize<'a> {
        let mut resp = self.relative_storage_request(Method::Get, path)?;
        let result: T = resp.json()?;
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

    pub fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration> {
        let server_config = self.fetch_info::<InfoConfiguration>("info/configuration")?;
        // TODO: Use default limits on 404, throw for everything else
        Ok(server_config)
    }

    pub fn fetch_meta_global(&self) -> error::Result<BsoRecord<MetaGlobalRecord>> {
        let mut resp = match self.relative_storage_request(Method::Get, "storage/meta/global") {
            Ok(r) => r,
            // This is gross, but at least it works. Replace 404s on meta/global with NoMetaGlobal.
            Err(error::Error(error::ErrorKind::StorageHttpError(StatusCode::NotFound, ..), _)) =>
                bail!(error::ErrorKind::NoMetaGlobal),
            Err(e) => return Err(e),
        };
        // Note: meta/global is not encrypted!
        let meta_global: BsoRecord<MetaGlobalRecord> = resp.json()?;
        Ok(meta_global)
    }

    // TODO: Can we deserialize this into a structure instead of a hash map, with known
    // keys and values? (Nested struct, since we don't support arbitrary collections, just
    // the ones that we can sync).
    pub fn fetch_info_collections(&self) -> error::Result<HashMap<String, ServerTimestamp>> {
        let payload = self.fetch_info::<HashMap<String, ServerTimestamp>>("info/collections")?;
        Ok(payload)
    }

    // These are encrypted with kSync.
    // And we use the per-collection encryption keys to encrypt and decrypt records for all other collections.
    pub fn fetch_crypto_keys(&self) -> error::Result<BsoRecord<EncryptedPayload>> {
        // TODO: if info/collections says we should, upload keys.
        // TODO: This should be handled in collection_keys.rs, which should track modified time, etc.
        let mut keys_resp = self.relative_storage_request(Method::Get, "storage/crypto/keys")?;
        let keys: BsoRecord<EncryptedPayload> = keys_resp.json()?;
        Ok(keys)
    }

    pub fn fetch_full_collection(&self, collection: &str) -> error::Result<Vec<BsoRecord<EncryptedPayload>>> {
        let mut resp = self.collection_request(Method::Get, CollectionRequest::new(collection).full())?;
        let records: Vec<BsoRecord<EncryptedPayload>> = resp.json()?;
        Ok(records)
    }

    // ...
    pub fn post<T, O>(&self,
                  coll: &str,
                  bytes: &[u8],
                  xius: ServerTimestamp,
                  batch: Option<String>,
                  commit: bool) -> error::Result<PostResponse>
    {
        let url = CollectionRequest::new(coll.to_owned())
                                    .batch(batch)
                                    .commit(commit)
                                    .build_url(Url::parse(&self.tsc.api_endpoint()?)?)?;

        let mut req = self.build_request(Method::Post, url)?;
        req.headers_mut().set(header::ContentType::json());
        req.headers_mut().set(XIfUnmodifiedSince(xius));
        // It's very annoying that we need to copy the body here, the request
        // shouldn't need to take ownership of it...
        *req.body_mut() = Some(Vec::from(bytes).into());
        let mut resp = self.exec_request(req, false)?;
        Ok(PostResponse::from_response(&mut resp)?)
    }
}
