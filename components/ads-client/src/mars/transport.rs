/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::hash::Hash;
use std::time::Duration;

use viaduct::{Client, ClientSettings, Request, Response};

use super::error::{HTTPError, TransportError};
use crate::{
    http_cache::{HttpCache, RequestHash},
    telemetry::Telemetry,
    CachePolicy,
};

const OHTTP_CHANNEL_ID: &str = "ads-client";

/// Wait before the single retry attempt for fire-and-forget callback
/// requests. Short enough to feel responsive on mobile, long enough to
/// let a transient blip recover.
const CALLBACK_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Run `op` and, if it returns `Err`, sleep for `delay` and run it once
/// more. The second result is returned as-is.
fn retry_once<F, T, E>(delay: Duration, mut op: F) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    match op() {
        Ok(v) => Ok(v),
        Err(_) => {
            std::thread::sleep(delay);
            op()
        }
    }
}

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
        // Callback requests (impression/click/report) are fire-and-forget on
        // unreliable mobile networks. Retry once on a transient viaduct error
        // (timeout, connection reset, DNS) before giving up. HTTP status
        // errors are not retried — those are decided after this call returns.
        let response = retry_once(CALLBACK_RETRY_DELAY, || client.send_sync(request.clone()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn retry_once_returns_first_success_without_calling_again() {
        let calls = Cell::new(0);
        let result: Result<&'static str, ()> = retry_once(Duration::ZERO, || {
            calls.set(calls.get() + 1);
            Ok("ok")
        });
        assert_eq!(result, Ok("ok"));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn retry_once_retries_after_failure_and_returns_success() {
        let calls = Cell::new(0);
        let result: Result<&'static str, &'static str> = retry_once(Duration::ZERO, || {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err("transient")
            } else {
                Ok("ok")
            }
        });
        assert_eq!(result, Ok("ok"));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn retry_once_gives_up_after_second_failure() {
        let calls = Cell::new(0);
        let result: Result<(), &'static str> = retry_once(Duration::ZERO, || {
            calls.set(calls.get() + 1);
            Err("still broken")
        });
        assert_eq!(result, Err("still broken"));
        assert_eq!(calls.get(), 2);
    }
}
