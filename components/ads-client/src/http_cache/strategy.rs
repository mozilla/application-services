/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::cache_control::CacheControl;
use super::request_hash::RequestHash;
use super::store::HttpCacheStore;
use super::{CacheOutcome, HttpCacheSendResult};
use std::time::Duration;
use viaduct::{Client, Request};

pub struct CacheFirst {
    pub hash: RequestHash,
    pub request: Request,
    pub ttl: Duration,
}

impl CacheFirst {
    pub fn apply(self, client: &Client, store: &HttpCacheStore) -> HttpCacheSendResult {
        let mut outcomes = vec![];
        match store.lookup(&self.hash) {
            Ok(Some(response)) => return Ok((response, vec![CacheOutcome::Hit])),
            Err(e) => outcomes.push(CacheOutcome::LookupFailed(e)),
            Ok(None) => {}
        }

        let network = NetworkFirst {
            hash: self.hash,
            request: self.request,
            ttl: self.ttl,
        };
        let (response, mut network_outcomes) = network.apply(client, store)?;
        outcomes.append(&mut network_outcomes);
        Ok((response, outcomes))
    }
}

pub struct NetworkFirst {
    pub hash: RequestHash,
    pub request: Request,
    pub ttl: Duration,
}

impl NetworkFirst {
    pub fn apply(self, client: &Client, store: &HttpCacheStore) -> HttpCacheSendResult {
        let response = client.send_sync(self.request)?;
        let cache_control = CacheControl::from(&response);
        let outcome = if cache_control.should_cache() {
            let ttl = cache_control.effective_ttl(self.ttl);
            if ttl.is_zero() {
                return Ok((response, vec![CacheOutcome::NoCache]));
            }
            match store.store_with_ttl(&self.hash, &response, &ttl) {
                Ok(()) => CacheOutcome::MissStored,
                Err(e) => CacheOutcome::StoreFailed(e),
            }
        } else {
            CacheOutcome::MissNotCacheable
        };
        Ok((response, vec![outcome]))
    }
}
