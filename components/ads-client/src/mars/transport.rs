/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{hash::Hash, sync::Arc};

use parking_lot::Mutex;
use viaduct::{Client, ClientSettings, Request, Response};

use super::error::{HTTPError, TransportError};
use crate::{
    http_cache::{HttpCache, RequestHash},
    telemetry::Telemetry,
    CachePolicy,
};

const OHTTP_CHANNEL_ID: &str = "ads-client";

pub struct MARSTransport<T: Telemetry> {
    http_cache: WrappedHttpCache,
    telemetry: T,
}

impl<T: Telemetry> MARSTransport<T> {
    pub fn new(http_cache: Option<HttpCache>, telemetry: T) -> Self {
        Self {
            http_cache: WrappedHttpCache::new(http_cache),
            telemetry,
        }
    }

    pub fn shutdown_db(&mut self) -> Result<(), rusqlite::Error> {
        if let Some(cache) = self.http_cache.take() {
            cache.shutdown_db()?;
        }
        Ok(())
    }

    pub fn clear_cache(&self) -> Result<(), rusqlite::Error> {
        if let Some(cache) = &self.http_cache.lock() {
            cache.clear()?;
        }
        Ok(())
    }

    pub fn fire(&self, request: Request, ohttp: bool) -> Result<(), TransportError> {
        let client = Self::client_for(ohttp)?;
        let response = client.send_sync(request)?;
        HTTPError::check(&response)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn invalidate_cache_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<(), rusqlite::Error> {
        if let Some(cache) = &self.http_cache.lock() {
            cache.invalidate_by_hash(request_hash)?;
        }
        Ok(())
    }

    pub fn send<R: Hash + Into<Request>>(
        &self,
        request: R,
        policy: &CachePolicy,
        ohttp: bool,
    ) -> Result<Response, TransportError> {
        let client = Self::client_for(ohttp)?;
        if let Some(cache) = &self.http_cache.lock() {
            let (response, outcomes) = cache.send_with_policy(&client, request, policy)?;
            for outcome in &outcomes {
                self.telemetry.record(outcome);
            }
            HTTPError::check(&response)?;
            Ok(response)
        } else {
            let response = client.send_sync(request.into())?;
            HTTPError::check(&response)?;
            Ok(response)
        }
    }

    fn client_for(ohttp: bool) -> Result<Client, viaduct::ViaductError> {
        if ohttp {
            Client::with_ohttp_channel(OHTTP_CHANNEL_ID, ClientSettings::default())
        } else {
            Ok(Client::new(ClientSettings::default()))
        }
    }

    #[cfg(test)]
    pub fn get_http_cache_lock(&self) -> WrappedHttpCache {
        self.http_cache.clone()
    }
}

#[derive(Clone)]
pub struct WrappedHttpCache(Option<Arc<Mutex<Option<HttpCache>>>>);
impl WrappedHttpCache {
    pub fn new(cache: Option<HttpCache>) -> WrappedHttpCache {
        if cache.is_some() {
            WrappedHttpCache(Some(Arc::new(Mutex::new(cache))))
        } else {
            WrappedHttpCache(None)
        }
    }

    pub fn take(&self) -> Option<HttpCache> {
        if let Some(cache) = &self.0 {
            let mut lock = cache.lock();
            lock.take()
        } else {
            None
        }
    }

    pub fn lock(
        &self,
    ) -> Option<parking_lot::lock_api::MappedMutexGuard<'_, parking_lot::RawMutex, HttpCache>> {
        if let Some(locked) = &self.0 {
            let lock: parking_lot::lock_api::MutexGuard<
                '_,
                parking_lot::RawMutex,
                Option<HttpCache>,
            > = locked.lock();
            let inner =
                parking_lot::lock_api::MutexGuard::try_map(lock, |x: &mut Option<HttpCache>| {
                    x.as_mut()
                })
                .ok()?;
            Some(inner)
        } else {
            None
        }
    }
}
