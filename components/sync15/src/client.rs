/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::bso_record::{BsoRecord, EncryptedBso};
use crate::error::{self, ErrorKind};
use crate::record_types::MetaGlobalRecord;
use crate::request::{
    BatchPoster, CollectionRequest, InfoCollections, InfoConfiguration, PostQueue, PostResponse,
    PostResponseHandler,
};
use crate::token;
use crate::util::ServerTimestamp;
use url::Url;
use viaduct::{
    header_names::{self, AUTHORIZATION},
    Method, Request, Response,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sync15StorageClientInit {
    pub key_id: String,
    pub access_token: String,
    pub tokenserver_url: Url,
}

/// A trait containing the methods required to run through the setup state
/// machine. This is factored out into a separate trait to make mocking
/// easier.
pub trait SetupStorageClient {
    fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration>;
    fn fetch_info_collections(&self) -> error::Result<InfoCollections>;
    fn fetch_meta_global(&self) -> error::Result<BsoRecord<MetaGlobalRecord>>;
    fn put_meta_global(&self, global: &BsoRecord<MetaGlobalRecord>) -> error::Result<()>;
    fn fetch_crypto_keys(&self) -> error::Result<EncryptedBso>;
    fn put_crypto_keys(&self, keys: &EncryptedBso) -> error::Result<()>;
    fn wipe_all_remote(&self) -> error::Result<()>;
}

#[derive(Debug)]
pub struct Sync15StorageClient {
    tsc: token::TokenProvider,
}

impl SetupStorageClient for Sync15StorageClient {
    fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration> {
        let server_config = self.fetch_info::<InfoConfiguration>("info/configuration")?;
        Ok(server_config)
    }

    fn fetch_info_collections(&self) -> error::Result<InfoCollections> {
        let collections = self.fetch_info::<InfoCollections>("info/collections")?;
        Ok(collections)
    }

    fn fetch_meta_global(&self) -> error::Result<BsoRecord<MetaGlobalRecord>> {
        let resp = match self.relative_storage_request(Method::Get, "storage/meta/global") {
            Ok(r) => Ok(r),
            Err(ref e) if e.is_not_found() => Err(ErrorKind::NoMetaGlobal.into()),
            Err(e) => Err(e),
        }?;
        // Note: meta/global is not encrypted!
        let meta_global: BsoRecord<MetaGlobalRecord> = resp.json()?;
        log::trace!("Meta global: {:?}", meta_global.payload);
        Ok(meta_global)
    }

    fn put_meta_global(&self, global: &BsoRecord<MetaGlobalRecord>) -> error::Result<()> {
        self.put("storage/meta/global", None, global)
    }

    fn fetch_crypto_keys(&self) -> error::Result<EncryptedBso> {
        let keys_resp = self.relative_storage_request(Method::Get, "storage/crypto/keys")?;
        let keys: EncryptedBso = keys_resp.json()?;
        Ok(keys)
    }

    fn put_crypto_keys(&self, keys: &EncryptedBso) -> error::Result<()> {
        self.put("storage/crypto/keys", None, keys)
    }

    fn wipe_all_remote(&self) -> error::Result<()> {
        let s = self.tsc.api_endpoint()?;
        let url = Url::parse(&s)?;

        let req = self.build_request(Method::Delete, url)?;
        match self.exec_request(req, true) {
            Ok(_) => Ok(()),
            Err(ref e) if e.is_not_found() => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl Sync15StorageClient {
    pub fn new(init_params: Sync15StorageClientInit) -> error::Result<Sync15StorageClient> {
        let tsc = token::TokenProvider::new(
            init_params.tokenserver_url,
            init_params.access_token,
            init_params.key_id,
        );
        Ok(Sync15StorageClient { tsc })
    }

    pub fn get_encrypted_records(
        &self,
        collection_request: &CollectionRequest,
    ) -> error::Result<Vec<EncryptedBso>> {
        let resp = self.collection_request(Method::Get, collection_request)?;
        Ok(resp.json()?)
    }

    #[inline]
    fn authorized(&self, req: Request) -> error::Result<Request> {
        let hawk_header_value = self.tsc.authorization(&req)?;
        Ok(req.header(AUTHORIZATION, hawk_header_value)?)
    }

    // TODO: probably want a builder-like API to do collection requests (e.g. something
    // that occupies roughly the same conceptual role as the Collection class in desktop)
    fn build_request(&self, method: Method, url: Url) -> error::Result<Request> {
        self.authorized(Request::new(method, url).header(header_names::ACCEPT, "application/json")?)
    }

    fn relative_storage_request<T>(
        &self,
        method: Method,
        relative_path: T,
    ) -> error::Result<Response>
    where
        T: AsRef<str>,
    {
        let s = self.tsc.api_endpoint()? + "/";
        let url = Url::parse(&s)?.join(relative_path.as_ref())?;
        Ok(self.make_storage_request(method, url)?)
    }

    fn make_storage_request(&self, method: Method, url: Url) -> error::Result<Response> {
        Ok(self.exec_request(self.build_request(method, url)?, true)?)
    }

    fn exec_request(&self, req: Request, require_success: bool) -> error::Result<Response> {
        log::trace!("request: {} {}", req.method, req.url.path());
        let resp = req.send()?;
        log::trace!("response: {}", resp.status);

        if require_success && !resp.is_success() {
            log::warn!(
                "HTTP error {} during storage request to {}",
                resp.status,
                resp.url.path()
            );
            return Err(ErrorKind::StorageHttpError {
                code: resp.status,
                route: resp.url.path().into(),
            }
            .into());
        }

        // TODO:
        // - handle backoff
        // - x-weave-quota?
        // - ... almost certainly other things too...

        Ok(resp)
    }

    fn collection_request(&self, method: Method, r: &CollectionRequest) -> error::Result<Response> {
        self.make_storage_request(method, r.build_url(Url::parse(&self.tsc.api_endpoint()?)?)?)
    }

    fn fetch_info<T>(&self, path: &str) -> error::Result<T>
    where
        for<'a> T: serde::de::Deserialize<'a>,
    {
        let resp = self.relative_storage_request(Method::Get, path)?;
        let result: T = resp.json()?;
        Ok(result)
    }

    pub fn new_post_queue<'a, F: PostResponseHandler>(
        &'a self,
        coll: &str,
        config: &InfoConfiguration,
        ts: ServerTimestamp,
        on_response: F,
    ) -> error::Result<PostQueue<PostWrapper<'a>, F>> {
        let pw = PostWrapper {
            client: self,
            coll: coll.into(),
        };
        Ok(PostQueue::new(config, ts, pw, on_response))
    }

    fn put<P, B>(
        &self,
        relative_path: P,
        xius: Option<ServerTimestamp>,
        body: &B,
    ) -> error::Result<()>
    where
        P: AsRef<str>,
        B: serde::ser::Serialize,
    {
        let s = self.tsc.api_endpoint()? + "/";
        let url = Url::parse(&s)?.join(relative_path.as_ref())?;

        let mut req = self.build_request(Method::Put, url)?.json(body);

        if let Some(ts) = xius {
            req = req.header(header_names::X_IF_UNMODIFIED_SINCE, format!("{}", ts))?;
        }

        let _ = self.exec_request(req, true)?;

        Ok(())
    }

    pub fn hashed_uid(&self) -> error::Result<String> {
        self.tsc.hashed_uid()
    }
}

pub struct PostWrapper<'a> {
    client: &'a Sync15StorageClient,
    coll: String,
}

impl<'a> BatchPoster for PostWrapper<'a> {
    fn post<T, O>(
        &self,
        bytes: Vec<u8>,
        xius: ServerTimestamp,
        batch: Option<String>,
        commit: bool,
        _: &PostQueue<T, O>,
    ) -> error::Result<PostResponse> {
        let url = CollectionRequest::new(self.coll.clone())
            .batch(batch)
            .commit(commit)
            .build_url(Url::parse(&self.client.tsc.api_endpoint()?)?)?;

        let req = self
            .client
            .build_request(Method::Post, url)?
            .header(header_names::CONTENT_TYPE, "application/json")?
            .header(header_names::X_IF_UNMODIFIED_SINCE, format!("{}", xius))?
            .body(bytes);
        let resp = self.client.exec_request(req, false)?;
        Ok(PostResponse::from_response(&resp)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_send() {
        fn ensure_send<T: Send>() {}
        // Compile will fail if not send.
        ensure_send::<Sync15StorageClient>();
    }
}
