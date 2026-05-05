/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod error;
mod http;
mod schema;
#[cfg(test)]
mod tests;

use error_support::handle_error;
use url::Url;

pub use error::{ApiResult, Error, MerinoWorldCupApiError, Result};
pub use schema::{WorldCupConfig, WorldCupOptions};

pub(crate) const DEFAULT_BASE_URL: &str = "https://merino.services.mozilla.com/api/v1/wcs/";

/// A client for the merino wcs endpoint.
///
/// Use [`WorldCupClient::new`] to create an instance, then call
/// [`WordCupClient::get_*`] to fetch wcs content.
#[derive(uniffi::Object)]
pub struct WorldCupClient {
    inner: WorldCupClientInner<http::HttpClient>,
    base_url: Url,
}

struct WorldCupClientInner<T: http::HttpClientTrait> {
    http_client: T,
}

#[derive(Default)]
struct WorldCupClientBuilder {
    base_host: Option<String>,
}

impl WorldCupClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base_host(mut self, base_host: String) -> Self {
        self.base_host = Some(base_host);
        self
    }

    pub fn build(self) -> Result<WorldCupClient> {
        let base_url = match self.base_host {
            Some(host) => Url::parse(&format!("{}/api/v1/wcs/", host))?,
            None => Url::parse(DEFAULT_BASE_URL)?,
        };

        Ok(WorldCupClient {
            inner: WorldCupClientInner::new()?,
            base_url,
        })
    }
}

#[uniffi::export]
impl WorldCupClient {
    /// Creates a new `WorldCupClient` from the given configuration.
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(config: WorldCupConfig) -> ApiResult<Self> {
        let mut builder = WorldCupClientBuilder::new();

        if let Some(host) = config.base_host {
            builder = builder.base_host(host);
        }

        builder.build()
    }

    #[handle_error(Error)]
    /// Fetches teams from the merino wcs endpoint
    pub fn get_teams(&self, options: WorldCupOptions) -> ApiResult<Option<String>> {
        let url = self.base_url.join("teams")?;
        let response = self.inner.make_request(url, options)?;
        Ok(response.map(|r| r.text().to_string()))
    }

    #[handle_error(Error)]
    /// Fetches matches from merino wcs endpoint
    pub fn get_matches(&self, options: WorldCupOptions) -> ApiResult<Option<String>> {
        let url = self.base_url.join("matches")?;
        let response = self.inner.make_request(url, options)?;
        Ok(response.map(|r| r.text().to_string()))
    }

    #[handle_error(Error)]
    /// Fetches live info from merino wcs endpoint
    pub fn get_live(&self, options: WorldCupOptions) -> ApiResult<Option<String>> {
        let url = self.base_url.join("live")?;
        let response = self.inner.make_request(url, options)?;
        Ok(response.map(|r| r.text().to_string()))
    }
}

impl WorldCupClientInner<http::HttpClient> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http_client: http::HttpClient,
        })
    }
}

impl<T: http::HttpClientTrait> WorldCupClientInner<T> {
    fn params(options: WorldCupOptions) -> http::WorldCupQueryParams {
        let teams = options
            .teams
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| v.join(","));
        http::WorldCupQueryParams {
            limit: options.limit,
            teams,
            accept_language: options.accept_language,
        }
    }

    pub fn make_request(
        &self,
        url: Url,
        options: WorldCupOptions,
    ) -> Result<Option<viaduct::Response>> {
        self.http_client.make_request(url, Self::params(options))
    }
}

#[cfg(test)]
impl<T: http::HttpClientTrait> WorldCupClientInner<T> {
    pub fn new_with_client(client: T) -> Self {
        Self {
            http_client: client,
        }
    }
}
