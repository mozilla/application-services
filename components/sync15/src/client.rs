/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::bso_record::EncryptedBso;
use crate::error::{self, ErrorKind};
use crate::record_types::MetaGlobalRecord;
use crate::request::{
    BatchPoster, CollectionRequest, InfoCollections, InfoConfiguration, PostQueue, PostResponse,
    PostResponseHandler, X_IF_UNMODIFIED_SINCE, X_LAST_MODIFIED,
};
use crate::token;
use crate::util::ServerTimestamp;
use hyper::Method;
use reqwest::{
    header::{self, HeaderValue, ACCEPT, AUTHORIZATION},
    Client, Request, Response, Url,
};
use serde_derive::*;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sync15StorageClientInit {
    pub key_id: String,
    pub access_token: String,
    pub tokenserver_url: Url,
}

fn get_response_timestamp(resp: &Response) -> error::Result<ServerTimestamp> {
    Ok(resp
        .headers()
        .get(X_LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| ServerTimestamp::from_str(s).ok())
        .ok_or_else(|| ErrorKind::MissingServerTimestamp)?)
}

/// A TimestampedResponse is used as a wrapper for any non-collection response
/// we may want to update with an x-if-unmodified-since header to ensure we are
/// overwriting what we think we are.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TimestampedResponse<T> {
    pub last_modified: ServerTimestamp,
    pub record: T,
}

/// A trait containing the methods required to run through the setup state
/// machine. This is factored out into a separate trait to make mocking
/// easier.
pub trait SetupStorageClient {
    fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration>;
    fn fetch_info_collections(&self) -> error::Result<TimestampedResponse<InfoCollections>>;
    fn fetch_meta_global(&self) -> error::Result<TimestampedResponse<MetaGlobalRecord>>;
    fn put_meta_global(
        &self,
        xius: ServerTimestamp,
        global: &MetaGlobalRecord,
    ) -> error::Result<()>;
    fn fetch_crypto_keys(&self) -> error::Result<TimestampedResponse<EncryptedBso>>;
    fn put_crypto_keys(&self, xius: ServerTimestamp, keys: &EncryptedBso) -> error::Result<()>;
    fn wipe_all_remote(&self) -> error::Result<()>;
}

#[derive(Debug)]
pub struct Sync15StorageClient {
    http_client: Client,
    tsc: token::TokenProvider,
}

impl SetupStorageClient for Sync15StorageClient {
    // we never update info/configuration, so there's no need to wrap in a TimestampedResponse.
    fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration> {
        let server_config = self.fetch_info::<InfoConfiguration>("info/configuration")?;
        Ok(server_config.record)
    }

    // we do update info/collections, so it's wrapped in a TimestampedResponse.
    fn fetch_info_collections(&self) -> error::Result<TimestampedResponse<InfoCollections>> {
        let collections = self.fetch_info::<InfoCollections>("info/collections")?;
        Ok(collections)
    }

    fn fetch_meta_global(&self) -> error::Result<TimestampedResponse<MetaGlobalRecord>> {
        let mut resp = match self.relative_storage_request(Method::GET, "storage/meta/global") {
            Ok(r) => Ok(r),
            Err(ref e) if e.is_not_found() => Err(ErrorKind::NoMetaGlobal.into()),
            Err(e) => Err(e),
        }?;
        // Note: meta/global is not encrypted!
        let meta_global: MetaGlobalRecord = resp.json()?;
        log::trace!("Meta global: {:?}", meta_global);

        let last_modified = get_response_timestamp(&resp)?;
        Ok(TimestampedResponse {
            last_modified,
            record: meta_global,
        })
    }

    fn put_meta_global(
        &self,
        xius: ServerTimestamp,
        global: &MetaGlobalRecord,
    ) -> error::Result<()> {
        self.put("storage/meta/global", xius, global)
    }

    fn fetch_crypto_keys(&self) -> error::Result<TimestampedResponse<EncryptedBso>> {
        let mut keys_resp = self.relative_storage_request(Method::GET, "storage/crypto/keys")?;
        let record: EncryptedBso = keys_resp.json()?;
        let last_modified = get_response_timestamp(&mut keys_resp)?;
        Ok(TimestampedResponse {
            last_modified,
            record,
        })
    }

    fn put_crypto_keys(&self, xius: ServerTimestamp, keys: &EncryptedBso) -> error::Result<()> {
        self.put("storage/crypto/keys", xius, keys)
    }

    fn wipe_all_remote(&self) -> error::Result<()> {
        let s = self.tsc.api_endpoint(&self.http_client)?;
        let url = Url::parse(&s)?;

        let req = self.build_request(Method::DELETE, url)?;
        match self.exec_request(req, true) {
            Ok(_) => Ok(()),
            Err(ref e) if e.is_not_found() => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl Sync15StorageClient {
    pub fn new(init_params: Sync15StorageClientInit) -> error::Result<Sync15StorageClient> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        let tsc = token::TokenProvider::new(
            init_params.tokenserver_url,
            init_params.access_token,
            init_params.key_id,
        );
        Ok(Sync15StorageClient {
            http_client: client,
            tsc,
        })
    }

    pub fn get_encrypted_records(
        &self,
        collection_request: &CollectionRequest,
    ) -> error::Result<Vec<EncryptedBso>> {
        let mut resp = self.collection_request(Method::GET, collection_request)?;
        Ok(resp.json()?)
    }

    #[inline]
    fn authorized(&self, mut req: Request) -> error::Result<Request> {
        let hawk_header_value = self.tsc.authorization(&self.http_client, &req)?;
        req.headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_str(&hawk_header_value)?);
        Ok(req)
    }

    // TODO: probably want a builder-like API to do collection requests (e.g. something
    // that occupies roughly the same conceptual role as the Collection class in desktop)
    fn build_request(&self, method: Method, url: Url) -> error::Result<Request> {
        self.authorized(
            self.http_client
                .request(method, url)
                .header(ACCEPT, "application/json")
                .build()?,
        )
    }

    fn relative_storage_request<T>(
        &self,
        method: Method,
        relative_path: T,
    ) -> error::Result<Response>
    where
        T: AsRef<str>,
    {
        let s = self.tsc.api_endpoint(&self.http_client)? + "/";
        let url = Url::parse(&s)?.join(relative_path.as_ref())?;
        Ok(self.make_storage_request(method, url)?)
    }

    fn make_storage_request(&self, method: Method, url: Url) -> error::Result<Response> {
        // I'm shocked that method isn't Copy...
        Ok(self.exec_request(self.build_request(method.clone(), url)?, true)?)
    }

    fn exec_request(&self, req: Request, require_success: bool) -> error::Result<Response> {
        log::trace!("request: {} {}", req.method(), req.url().path());
        let resp = self.http_client.execute(req)?;
        log::trace!("response: {}", resp.status());

        if require_success && !resp.status().is_success() {
            log::warn!(
                "HTTP error {} ({}) during storage request to {}",
                resp.status().as_u16(),
                resp.status(),
                resp.url().path()
            );
            return Err(ErrorKind::StorageHttpError {
                code: resp.status().as_u16(),
                route: resp.url().path().into(),
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
        self.make_storage_request(
            method.clone(),
            r.build_url(Url::parse(&self.tsc.api_endpoint(&self.http_client)?)?)?,
        )
    }

    fn fetch_info<T>(&self, path: &str) -> error::Result<TimestampedResponse<T>>
    where
        for<'a> T: serde::de::Deserialize<'a>,
    {
        let mut resp = self.relative_storage_request(Method::GET, path)?;
        let record: T = resp.json()?;
        let last_modified = get_response_timestamp(&mut resp)?;
        Ok(TimestampedResponse {
            last_modified,
            record,
        })
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

    fn put<P, B>(&self, relative_path: P, xius: ServerTimestamp, body: &B) -> error::Result<()>
    where
        P: AsRef<str>,
        B: serde::ser::Serialize,
    {
        let s = self.tsc.api_endpoint(&self.http_client)? + "/";
        let url = Url::parse(&s)?.join(relative_path.as_ref())?;

        let bytes = serde_json::to_vec(body)?;

        let mut req = self.build_request(Method::PUT, url)?;
        req.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        req.headers_mut().insert(
            X_IF_UNMODIFIED_SINCE,
            HeaderValue::from_str(&format!("{}", xius))?,
        );
        *req.body_mut() = Some(bytes.into());
        let _ = self.exec_request(req, true)?;

        Ok(())
    }

    pub fn hashed_uid(&self) -> error::Result<String> {
        self.tsc.hashed_uid(&self.http_client)
    }
}

pub struct PostWrapper<'a> {
    client: &'a Sync15StorageClient,
    coll: String,
}

impl<'a> BatchPoster for PostWrapper<'a> {
    fn post<T, O>(
        &self,
        bytes: &[u8],
        xius: ServerTimestamp,
        batch: Option<String>,
        commit: bool,
        _: &PostQueue<T, O>,
    ) -> error::Result<PostResponse> {
        let url = CollectionRequest::new(self.coll.clone())
            .batch(batch)
            .commit(commit)
            .build_url(Url::parse(
                &self.client.tsc.api_endpoint(&self.client.http_client)?,
            )?)?;

        let mut req = self.client.build_request(Method::POST, url)?;
        req.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        req.headers_mut().insert(
            X_IF_UNMODIFIED_SINCE,
            HeaderValue::from_str(&format!("{}", xius))?,
        );
        // It's very annoying that we need to copy the body here, the request
        // shouldn't need to take ownership of it...
        *req.body_mut() = Some(Vec::from(bytes).into());
        let mut resp = self.client.exec_request(req, false)?;
        Ok(PostResponse::from_response(&mut resp)?)
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
