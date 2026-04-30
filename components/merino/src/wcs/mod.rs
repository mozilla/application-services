/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Client for fetching World Cup Soccer data from the Merino service.
//!
//! This module provides [`WcsClient`], which supports two endpoints:
//! - `GET /api/v1/wcs/live` — currently in-progress matches
//! - `GET /api/v1/wcs/matches` — previous, current, and upcoming matches for a given date

mod error;
mod http;
pub mod models;
#[cfg(test)]
mod tests;

use error_support::handle_error;
use models::request::{WcsConfig, WcsLiveOptions, WcsMatchesOptions, WcsRetryConfig};
use models::response::{WcsLiveMatchesResponse, WcsMatchesResponse};
use url::Url;

pub use error::{ApiResult, Error, Result, WcsApiError};

const DEFAULT_BASE_HOST: &str = "https://merino.services.mozilla.com";

/// Client for fetching World Cup Soccer match data from the Merino service.
///
/// Construct using [`WcsClient::new`] with a [`WcsConfig`], then call:
/// - [`get_live_matches`](WcsClient::get_live_matches) for in-progress matches
/// - [`get_matches`](WcsClient::get_matches) for bucketed previous/current/next matches
///
/// Both methods transparently retry transient failures (5xx / network errors) according
/// to the [`WcsRetryConfig`] supplied in [`WcsConfig`].
#[derive(uniffi::Object)]
pub struct WcsClient {
    inner: WcsClientInner<http::HttpClient>,
    live_url: Url,
    matches_url: Url,
}

struct WcsClientInner<T: http::HttpClientTrait> {
    http_client: T,
    retry_config: WcsRetryConfig,
}

#[derive(Default)]
pub struct WcsClientBuilder {
    base_host: Option<String>,
    retry_config: Option<WcsRetryConfig>,
}

impl WcsClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base_host(mut self, base_host: impl Into<String>) -> Self {
        self.base_host = Some(base_host.into());
        self
    }

    pub fn retry_config(mut self, retry_config: WcsRetryConfig) -> Self {
        self.retry_config = Some(retry_config);
        self
    }

    pub fn build(self) -> Result<WcsClient> {
        let base_host = self
            .base_host
            .unwrap_or_else(|| DEFAULT_BASE_HOST.to_string());
        let live_url = Url::parse(&format!("{}/api/v1/wcs/live", base_host))?;
        let matches_url = Url::parse(&format!("{}/api/v1/wcs/matches", base_host))?;
        let retry_config = self
            .retry_config
            .unwrap_or_else(WcsRetryConfig::default_config);

        Ok(WcsClient {
            inner: WcsClientInner::new(retry_config)?,
            live_url,
            matches_url,
        })
    }
}

#[uniffi::export]
impl WcsClient {
    /// Creates a new `WcsClient` from the given configuration.
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(config: WcsConfig) -> ApiResult<Self> {
        let mut builder = WcsClientBuilder::new();
        if let Some(base_host) = config.base_host {
            builder = builder.base_host(base_host);
        }
        if let Some(retry_config) = config.retry_config {
            builder = builder.retry_config(retry_config);
        }
        builder.build()
    }

    /// Fetches currently live World Cup matches (`GET /api/v1/wcs/live`).
    ///
    /// Results can be filtered by team using [`WcsLiveOptions::teams`].
    /// Retries automatically on transient failures per the configured [`WcsRetryConfig`].
    #[handle_error(Error)]
    pub fn get_live_matches(&self, options: WcsLiveOptions) -> ApiResult<WcsLiveMatchesResponse> {
        self.inner.get_live_matches(options, &self.live_url)
    }

    /// Fetches World Cup matches bucketed into previous, current, and next (`GET /api/v1/wcs/matches`).
    ///
    /// Defaults to today UTC. Supports optional date, per-bucket limit, and team filter
    /// via [`WcsMatchesOptions`]. Retries automatically on transient failures.
    #[handle_error(Error)]
    pub fn get_matches(&self, options: WcsMatchesOptions) -> ApiResult<WcsMatchesResponse> {
        self.inner.get_matches(options, &self.matches_url)
    }
}

impl WcsClientInner<http::HttpClient> {
    pub fn new(retry_config: WcsRetryConfig) -> Result<Self> {
        Ok(Self {
            http_client: http::HttpClient,
            retry_config,
        })
    }
}

impl<T: http::HttpClientTrait> WcsClientInner<T> {
    /// Calls `f` up to `1 + max_retries` times, sleeping with exponential backoff between
    /// attempts. Only retries on errors where [`Error::is_retryable`] returns `true`.
    fn with_retry<R, F>(&self, f: F) -> Result<R>
    where
        F: Fn() -> Result<R>,
    {
        let mut delay_ms = self.retry_config.initial_delay_ms;
        for attempt in 0..=self.retry_config.max_retries {
            match f() {
                Ok(v) => return Ok(v),
                Err(e) if e.is_retryable() && attempt < self.retry_config.max_retries => {
                    if delay_ms > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    }
                    delay_ms = (delay_ms.saturating_mul(2)).min(self.retry_config.max_delay_ms);
                }
                Err(e) => return Err(e),
            }
        }
        // Unreachable: the loop always returns in the last iteration (attempt == max_retries
        // means the `attempt < max_retries` guard is false, so we fall through to `Err(e)`).
        unreachable!()
    }

    pub fn get_live_matches(
        &self,
        options: WcsLiveOptions,
        endpoint_url: &Url,
    ) -> Result<WcsLiveMatchesResponse> {
        let teams = options
            .teams
            .filter(|v| !v.is_empty())
            .map(|v| v.join(","));
        self.with_retry(|| {
            self.http_client
                .make_live_matches_request(teams.as_deref(), endpoint_url.clone())
        })
    }

    pub fn get_matches(
        &self,
        options: WcsMatchesOptions,
        endpoint_url: &Url,
    ) -> Result<WcsMatchesResponse> {
        let teams = options
            .teams
            .filter(|v| !v.is_empty())
            .map(|v| v.join(","));
        self.with_retry(|| {
            self.http_client.make_matches_request(
                options.date.as_deref(),
                options.limit,
                teams.as_deref(),
                endpoint_url.clone(),
            )
        })
    }
}

#[cfg(test)]
impl<T: http::HttpClientTrait> WcsClientInner<T> {
    pub fn new_with_client(client: T) -> Self {
        Self {
            http_client: client,
            retry_config: WcsRetryConfig::no_retry(),
        }
    }

    pub fn new_with_client_and_retry(client: T, retry_config: WcsRetryConfig) -> Self {
        Self {
            http_client: client,
            retry_config,
        }
    }
}
