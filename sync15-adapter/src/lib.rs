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
extern crate serde_derive;

#[macro_use]
extern crate log;

extern crate serde_json;

#[macro_use]
extern crate error_chain;

// TODO: Some of these don't need to be pub...
pub mod key_bundle;
pub mod error;
pub mod bso_record;
pub mod record_types;
pub mod token;
pub mod collection_keys;
pub mod util;

pub use MaybeTombstone::*;


use std::cell::Cell;
use std::time::{Duration};

use key_bundle::KeyBundle;
use reqwest::{
    Client,
    Request,
    Response,
    Url,
    header::Accept
};
use hyper::Method;
use bso_record::{BsoRecord, Sync15Record, EncryptedPayload};
use record_types::MaybeTombstone;
use collection_keys::CollectionKeys;

// Storage server's timestamp
header! { (XWeaveTimestamp, "X-Weave-Timestamp") => [f64] }

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
    last_server_time: Cell<f64>,
    tsc: token::TokenserverClient,
    keys: Option<CollectionKeys>,
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
    fn storage_request<T>(&self, method: Method, relative_path: T) -> error::Result<Request> where T: AsRef<str> {
        let url = Url::parse(&self.tsc.token().api_endpoint)?.join(relative_path.as_ref())?;
        self.authorized(self.client.request(method, url).header(Accept::json()).build()?)
    }

    fn make_storage_request<T>(&self, method: Method, relative_path: T) -> error::Result<Response> where T: AsRef<str> {
        let resp = self.client.execute(self.storage_request(method, relative_path)?)?;
        // TODO:
        // - handle http errors...
        // - record x-weave-timestamp
        // - handle backoff
        // - x-weave-quota?
        // - ... almost certainly other things too...
        Ok(resp)
    }

    pub fn fetch_keys(&mut self) -> error::Result<()> {
        let mut keys_resp = self.make_storage_request(Method::Get, "storage/crypto/keys")?;
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
        let mut resp = self.make_storage_request(Method::Get, format!("storage/{}?full=1", collection))?;
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
}

