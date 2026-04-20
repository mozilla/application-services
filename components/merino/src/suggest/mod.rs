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

pub use error::{ApiResult, Error, MerinoSuggestApiError, Result};
pub use schema::{SuggestConfig, SuggestOptions};

const DEFAULT_BASE_HOST: &str = "https://merino.services.mozilla.com";

/// A client for the merino suggest endpoint.
///
/// Use [`SuggestClient::new`] to create an instance, then call
/// [`SuggestClient::get_suggestions`] to fetch suggestions for a query.
#[derive(uniffi::Object)]
pub struct SuggestClient {
    inner: SuggestClientInner<http::HttpClient>,
    endpoint_url: Url,
}

struct SuggestClientInner<T: http::HttpClientTrait> {
    http_client: T,
}

#[derive(Default)]
pub struct SuggestClientBuilder {
    base_host: Option<String>,
}

impl SuggestClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base_host(mut self, base_host: String) -> Self {
        self.base_host = Some(base_host);
        self
    }

    pub fn build(self) -> Result<SuggestClient> {
        let base_host = self
            .base_host
            .unwrap_or_else(|| DEFAULT_BASE_HOST.to_string());

        let url = format!("{}/api/v1/suggest", base_host);
        let endpoint_url = Url::parse(&url)?;

        Ok(SuggestClient {
            inner: SuggestClientInner::new()?,
            endpoint_url,
        })
    }
}

#[uniffi::export]
impl SuggestClient {
    /// Creates a new `SuggestClient` from the given configuration.
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(config: SuggestConfig) -> ApiResult<Self> {
        let mut builder = SuggestClientBuilder::new();

        if let Some(base_host) = config.base_host {
            builder = builder.base_host(base_host);
        }

        builder.build()
    }

    /// Fetches suggestions from the merino suggest endpoint for the given query.
    ///
    /// Returns the raw JSON response body as a string, or `None` if the server
    /// returned HTTP 204 (no suggestions available for weather).
    #[handle_error(Error)]
    pub fn get_suggestions(
        &self,
        query: String,
        options: SuggestOptions,
    ) -> ApiResult<Option<String>> {
        let response = self
            .inner
            .get_suggestions(query.as_str(), options, &self.endpoint_url)?;

        Ok(response.map(|r| r.text().to_string()))
    }
}

impl SuggestClientInner<http::HttpClient> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http_client: http::HttpClient,
        })
    }
}

impl<T: http::HttpClientTrait> SuggestClientInner<T> {
    pub fn get_suggestions(
        &self,
        query: &str,
        options: SuggestOptions,
        endpoint_url: &Url,
    ) -> Result<Option<viaduct::Response>> {
        let providers = options
            .providers
            .filter(|v| !v.is_empty())
            .map(|v| v.join(","));
        let client_variants = options
            .client_variants
            .filter(|v| !v.is_empty())
            .map(|v| v.join(","));
        self.http_client.make_suggest_request(
            query,
            http::SuggestQueryParams {
                providers: providers.as_deref(),
                source: options.source.as_deref(),
                country: options.country.as_deref(),
                region: options.region.as_deref(),
                city: options.city.as_deref(),
                client_variants: client_variants.as_deref(),
                request_type: options.request_type.as_deref(),
                accept_language: options.accept_language.as_deref(),
            },
            endpoint_url.clone(),
        )
    }
}

#[cfg(test)]
impl<T: http::HttpClientTrait> SuggestClientInner<T> {
    pub fn new_with_client(client: T) -> Self {
        Self {
            http_client: client,
        }
    }
}
