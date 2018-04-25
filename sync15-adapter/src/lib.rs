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

pub mod key_bundle;
pub mod error;
pub mod bso_record;
pub mod record_types;

use std::collections::HashMap;
use std::cell::Cell;
use std::time::{Duration};

use key_bundle::KeyBundle;
use reqwest::{
    Client,
    Request,
    Response,
    Url,
    header::{Authorization, Bearer, Accept}
};
use hyper::Method;
use bso_record::{BsoRecord, Sync15Record, MaybeTombstone, EncryptedPayload};

header! { (XKeyID, "X-KeyID") => [String] }
header! { (RetryAfter, "Retry-After") => [f64] }

// Tokenserver's timestamp
header! { (XTimestamp, "X-Timestamp") => [f64] }
// Storage server's timestamp
header! { (XWeaveTimestamp, "X-Weave-Timestamp") => [f64] }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sync15ServiceInit {
    pub key_id: String,
    pub access_token: String,
    pub sync_key: String,
    pub tokenserver_base_url: String,
}

#[derive(Deserialize, Clone, Debug, Eq, PartialEq)]
struct TokenserverToken {
    pub id: String,
    pub key: String,
    pub api_endpoint: String,
    pub uid: u64,
    pub duration: u64,
    pub hashed_fxa_uid: Option<String>,
}

impl TokenserverToken {
    pub fn authorization(&self, req: &Request) -> error::Result<Authorization<String>> {
        let url = req.url();
        let path_and_query = match url.query() {
            None => url.path().into(),
            Some(qs) => format!("{}?{}", url.path(), qs)
        };

        let creds = hawk::Credentials {
            id: self.id.clone(),
            key: hawk::Key::new(self.key.as_bytes(), &hawk::SHA256),
        };

        let host = url.host_str().ok_or_else(||
            error::unexpected("Tried to authorize bad URL using hawk (no host)"))?;

        let port = url.port_or_known_default().ok_or_else(||
            error::unexpected("Tried to authorize bad URL using hawk (no port)"))?;


        let header = hawk::RequestBuilder::new(
            req.method().as_ref(), host, port, &path_and_query
        ).request().make_header(&creds)?;

        Ok(Authorization(format!("Hawk {}", header)))
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
struct CryptoKeysRecord {
    pub id: String,
    pub default: Vec<String>,
    pub collection: String, // "crypto",
    pub collections: HashMap<String, Vec<String>>
}

impl Sync15Record for CryptoKeysRecord {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CollectionKeys {
    pub default: KeyBundle,
    pub collections: HashMap<String, KeyBundle>
}

impl CollectionKeys {

    pub fn from_bso(record: BsoRecord<CryptoKeysRecord>) -> error::Result<CollectionKeys> {
        Ok(CollectionKeys {
            default: KeyBundle::from_base64(&record.payload.default[0], &record.payload.default[1])?,
            collections:
                record.payload.collections
                              .iter()
                              .map(|kv| Ok((kv.0.clone(), KeyBundle::from_base64(&kv.1[0], &kv.1[1])?)))
                              .collect::<error::Result<HashMap<String, KeyBundle>>>()?
        })
    }

    pub fn to_bso(&self) -> BsoRecord<CryptoKeysRecord> {
        BsoRecord {
            id: "keys".into(),
            collection: Some("crypto".into()),
            modified: 0.0, // ignored
            sortindex: None,
            ttl: None,
            payload: CryptoKeysRecord {
                id: "keys".into(),
                collection: "crypto".into(),
                default: self.default.to_b64_vec(),
                collections: self.collections.iter().map(|kv|
                    (kv.0.clone(), kv.1.to_b64_vec())).collect()
            },
        }
    }

    #[inline]
    pub fn key_for_collection<'a>(&'a self, collection: &str) -> &'a KeyBundle {
        self.collections.get(collection).unwrap_or(&self.default)
    }
}

#[derive(Debug, Clone)]
pub struct Sync15Service {
    init_params: Sync15ServiceInit,
    tokenserver_url: reqwest::Url,
    root_key: KeyBundle,
    client: Client,
    last_server_time: Cell<f64>,
    token: Option<TokenserverToken>,
    keys: Option<CollectionKeys>,
}

impl Sync15Service {
    pub fn new(init_params: Sync15ServiceInit) -> error::Result<Sync15Service> {
        let url = init_params.tokenserver_base_url.clone() + "/1.0/sync/1.5";
        let root_key = KeyBundle::from_ksync_base64(&init_params.sync_key)?;
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Sync15Service {
            tokenserver_url: Url::parse(&url)?,
            client,
            init_params,
            root_key,
            last_server_time: Cell::new(0.0),
            token: None,
            keys: None,
        })
    }

    fn fetch_token(&self) -> error::Result<TokenserverToken> {
        let mut resp = self.client
            .get(self.tokenserver_url.clone())
            .header(Authorization(Bearer { token: self.init_params.access_token.clone() }))
            .header(XKeyID(self.init_params.key_id.clone()))
            .send()?;
        if !resp.status().is_success() {
            warn!("Non-success status when fetching token: {}", resp.status());
            trace!("  Response body {}", resp.text().unwrap_or("???".into()));
            if let Some(seconds) = resp.headers().get::<RetryAfter>().map(|h| **h) {
                bail!(error::ErrorKind::BackoffError(seconds));
            }
            bail!(error::ErrorKind::TokenserverHttpError(resp.status()));
        }
        if let Some(timestamp) = resp.headers().get::<XTimestamp>().map(|h| **h) {
            self.last_server_time.set(timestamp);
        } else {
            bail!(error::unexpected("Missing or corrupted X-Timestamp header from token server"));
        }
        Ok(resp.json()?)
    }

    #[inline]
    fn authorization(&self, req: &Request) -> error::Result<Authorization<String>> {
        self.get_token()?.authorization(req)
    }

    #[inline]
    fn authorized(&self, mut req: Request) -> error::Result<Request> {
        let header = self.authorization(&req)?;
        req.headers_mut().set(header);
        Ok(req)
    }

    #[inline]
    fn get_token(&self) -> error::Result<&TokenserverToken> {
        // TODO: expiration, etc
        Ok(self.token.as_ref().ok_or_else(|| error::unexpected("Don't have token."))?)
    }

    // TODO: probably want a builder-like API to do collection requests (e.g. something
    // that occupies roughly the same conceptual role as the Collection class in desktop)
    fn storage_request<T>(&self, method: Method, relative_path: T) -> error::Result<Request> where T: AsRef<str> {
        let url = Url::parse(&self.get_token()?.api_endpoint)?.join(relative_path.as_ref())?;
        self.authorized(self.client.request(method, url).header(Accept::json()).build()?)
    }

    fn make_storage_request<T>(&self, method: Method, relative_path: T) -> error::Result<Response> where T: AsRef<str> {
        let resp = self.client.execute(self.storage_request(method, relative_path)?)?;
        // TODO:
        // - handle keys_resp errors...
        // - record x-weave-timestamp
        // - handle backoff
        // - x-weave-quota?
        // - ... almost certainly other things too...
        Ok(resp)
    }

    pub fn startup(&mut self) -> error::Result<()> {
        self.token = Some(self.fetch_token()?);
        let mut keys_resp = self.make_storage_request(Method::Get, "storage/crypto/keys")?;
        let keys: BsoRecord<EncryptedPayload> = keys_resp.json()?;
        self.keys = Some(CollectionKeys::from_bso(keys.decrypt_as(&self.root_key)?)?);
        // TODO: error handling and also this isn't nearly enough
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
            if let Some(record) = decrypted.with_record() {
                result.push(record);
            }
        }
        Ok(result)
    }
}

