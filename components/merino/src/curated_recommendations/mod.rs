/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

mod error;
mod http;
mod models;
#[cfg(test)]
mod tests;

use crate::curated_recommendations::models::CuratedRecommendationLocale;
pub use error::{ApiResult, Error, Result};
use error_support::handle_error;
pub use models::{
    CuratedRecommendationsConfig, CuratedRecommendationsRequest, CuratedRecommendationsResponse,
};
use url::Url;

const DEFAULT_BASE_HOST: &str = "https://merino.services.mozilla.com";

#[derive(uniffi::Object)]
pub struct CuratedRecommendationsClient {
    inner: CuratedRecommendationsClientInner<http::HttpClient>,
    endpoint_url: Url,
    user_agent_header: String,
}

struct CuratedRecommendationsClientInner<T: http::HttpClientTrait> {
    http_client: T,
}

#[derive(Default)]
pub struct CuratedRecommendationsClientBuilder {
    base_host: Option<String>,
    user_agent_header: Option<String>,
}

impl CuratedRecommendationsClientBuilder {
    pub fn new() -> Self {
        Self {
            base_host: None,
            user_agent_header: None,
        }
    }

    pub fn base_host(mut self, base_host: impl Into<String>) -> Self {
        self.base_host = Some(base_host.into());
        self
    }

    pub fn user_agent_header(mut self, user_agent_header: impl Into<String>) -> Self {
        self.user_agent_header = Some(user_agent_header.into());
        self
    }

    pub fn build(self) -> Result<CuratedRecommendationsClient> {
        let user_agent_header = self.user_agent_header.ok_or_else(|| Error::Unexpected {
            code: 0,
            message: "user_agent_header must be provided".to_string(),
        })?;

        let base_host = self
            .base_host
            .unwrap_or_else(|| DEFAULT_BASE_HOST.to_string());

        let url = format!("{}/api/v1/curated-recommendations", base_host);
        let endpoint_url = Url::parse(&url)?;

        Ok(CuratedRecommendationsClient {
            inner: CuratedRecommendationsClientInner::new()?,
            endpoint_url,
            user_agent_header,
        })
    }
}

#[uniffi::export]
impl CuratedRecommendationsClient {
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(config: CuratedRecommendationsConfig) -> ApiResult<Self> {
        let mut builder =
            CuratedRecommendationsClientBuilder::new().user_agent_header(config.user_agent_header);

        if let Some(base_host) = config.base_host {
            builder = builder.base_host(base_host);
        }

        builder.build()
    }

    #[handle_error(Error)]
    pub fn get_curated_recommendations(
        &self,
        request: &CuratedRecommendationsRequest,
    ) -> ApiResult<CuratedRecommendationsResponse> {
        self.inner
            .get_curated_recommendations(request, &self.user_agent_header, &self.endpoint_url)
    }
}
/// Parses a serialized locale string (e.g. `"en-US"`) into a `CuratedRecommendationLocale` enum variant.
///
///
/// Returns `None` if the string does not match any known locale.
#[uniffi::export]
pub fn curated_recommendation_locale_from_string(
    locale: String,
) -> Option<CuratedRecommendationLocale> {
    CuratedRecommendationLocale::from_locale_string(locale)
}

/// Returns a list of all supported locale strings that map to `CuratedRecommendationLocale` variants.
#[uniffi::export]
pub fn all_curated_recommendation_locales() -> Vec<String> {
    CuratedRecommendationLocale::all_locales()
}

impl CuratedRecommendationsClientInner<http::HttpClient> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http_client: http::HttpClient,
        })
    }
}

impl<T: http::HttpClientTrait> CuratedRecommendationsClientInner<T> {
    pub fn get_curated_recommendations(
        &self,
        request: &CuratedRecommendationsRequest,
        user_agent_header: &str,
        endpoint_url: &Url,
    ) -> Result<CuratedRecommendationsResponse> {
        self.http_client.make_curated_recommendation_request(
            request,
            user_agent_header,
            endpoint_url.clone(),
        )
    }
}

#[cfg(test)]
impl<T: http::HttpClientTrait> CuratedRecommendationsClientInner<T> {
    // allows us to inject a fake http client for testing
    pub fn new_with_client(client: T) -> Self {
        Self {
            http_client: client,
        }
    }
}
