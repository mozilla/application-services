/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::hash::Hash;

use viaduct::{Client, ClientSettings, Request, Response};

use super::error::{HTTPError, TransportError};
use crate::{
    http_cache::{HttpCache, RequestHash},
    telemetry::Telemetry,
    CachePolicy,
};

const OHTTP_CHANNEL_ID: &str = "ads-client";

pub struct MARSTransport<T: Telemetry> {
    http_cache: Option<HttpCache>,
    telemetry: T,
}

impl<T: Telemetry> MARSTransport<T> {
    pub fn new(http_cache: Option<HttpCache>, telemetry: T) -> Self {
        Self {
            http_cache,
            telemetry,
        }
    }

    pub fn clear_cache(&self) -> Result<(), rusqlite::Error> {
        if let Some(cache) = &self.http_cache {
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
        if let Some(cache) = &self.http_cache {
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
        if let Some(cache) = &self.http_cache {
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
}
