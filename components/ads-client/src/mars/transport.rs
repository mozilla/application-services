/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{hash::Hash, sync::Arc};

use sql_support::open_database;
use viaduct::{Client, ClientSettings, Request, Response};

use super::error::{HTTPError, TransportError};
use crate::{
    http_cache::{HttpCache, RequestHash},
    shutdown::HttpCacheShutdown,
    telemetry::Telemetry,
    CachePolicy,
};

const OHTTP_CHANNEL_ID: &str = "ads-client";

pub struct MARSTransport<T: Telemetry> {
    // TODO: Revert if we get rid of shutdown references.
    http_cache: Arc<Option<HttpCache>>,
    telemetry: T,
}

impl<T: Telemetry> MARSTransport<T> {
    pub fn new(http_cache: Option<HttpCache>, telemetry: T) -> Self {
        Self {
            http_cache: Arc::new(http_cache),
            telemetry,
        }
    }

    pub fn clear_cache(&self) -> Result<(), open_database::Error> {
        if let Some(cache) = self.http_cache.as_ref() {
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
    ) -> Result<(), open_database::Error> {
        if let Some(cache) = self.http_cache.as_ref() {
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
        if let Some(cache) = self.http_cache.as_ref() {
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

    pub fn shutdown_db(&mut self) {
        if let Some(cache) = self.http_cache.as_ref() {
            cache.shutdown_db();
        }
    }

    // TODO: Possibly remove
    pub fn get_http_cache_shutdown(&self) -> HttpCacheShutdown {
        HttpCacheShutdown::new(self.http_cache.clone())
    }

    fn client_for(ohttp: bool) -> Result<Client, viaduct::ViaductError> {
        if ohttp {
            Client::with_ohttp_channel(OHTTP_CHANNEL_ID, ClientSettings::default())
        } else {
            Ok(Client::new(ClientSettings::default()))
        }
    }
}
