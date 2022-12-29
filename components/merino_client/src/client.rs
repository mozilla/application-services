/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rc_crypto::rand;
use serde_derive::*;
use url::Url;
use viaduct::Request;

use crate::error::{MerinoClientError, MerinoClientResult};

pub struct MerinoClient {
    base_url: Url,
    session_duration: Duration,
    client_variants: Vec<String>,
    default_providers: Vec<String>,
    session_state: Arc<Mutex<SessionState>>,
}

impl MerinoClient {
    pub fn new(settings: MerinoClientSettings) -> MerinoClientResult<Self> {
        Ok(Self {
            base_url: settings.server.try_into()?,
            session_duration: Duration::from_millis(settings.session_duration_ms),
            client_variants: settings.client_variants,
            default_providers: settings.default_providers,
            session_state: Default::default(),
        })
    }

    fn session_params(&self) -> (String, i64) {
        let mut state = self.session_state.lock().unwrap();
        if state.started_at.elapsed() >= self.session_duration {
            *state = Default::default();
        }
        let result = (state.session_id.clone(), state.sequence_number);
        state.sequence_number += 1;
        result
    }

    pub fn fetch(
        &self,
        query: &str,
        options: Option<MerinoClientFetchOptions>,
    ) -> MerinoClientResult<Vec<MerinoSuggestion>> {
        let mut endpoint_url =
            self.base_url
                .join("/api/v1/suggest")
                .map_err(|err| MerinoClientError::BadUrl {
                    reason: err.to_string(),
                })?;
        let (session_id, sequence_number) = self.session_params();

        endpoint_url
            .query_pairs_mut()
            .append_pair("q", query)
            .append_pair("sid", &session_id)
            .append_pair("seq", &sequence_number.to_string());
        if !self.client_variants.is_empty() {
            endpoint_url
                .query_pairs_mut()
                .append_pair("client_variants", &self.client_variants.join(","));
        }

        if let Some(providers) = &options.and_then(|options| options.providers) {
            endpoint_url
                .query_pairs_mut()
                .append_pair("providers", &providers.join(","));
        } else if !self.default_providers.is_empty() {
            endpoint_url
                .query_pairs_mut()
                .append_pair("providers", &self.default_providers.join(","));
        }
        endpoint_url.query_pairs_mut().finish();

        let request = Request::get(endpoint_url);
        let response: SuggestResponse = request
            .send()
            .map_err(|err| MerinoClientError::FetchFailed {
                reason: err.to_string(),
            })?
            .require_success()
            .map_err(|err| MerinoClientError::FetchFailed {
                reason: err.to_string(),
            })?
            .json()
            .map_err(|err| MerinoClientError::FetchFailed {
                reason: err.to_string(),
            })?;

        Ok(response
            .suggestions
            .into_iter()
            .map(|suggestion| MerinoSuggestion {
                title: suggestion.title,
                url: suggestion.url,
                provider: suggestion.provider,
                is_sponsored: suggestion.is_sponsored,
                score: suggestion.score,
                icon: suggestion.icon,
                request_id: response.request_id.clone(),
            })
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerinoClientSettings {
    pub server: MerinoServer,
    pub session_duration_ms: u64,
    pub client_variants: Vec<String>,
    pub default_providers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MerinoServer {
    Production,
    Stage,
    Custom { url: String },
}

impl TryFrom<MerinoServer> for Url {
    type Error = MerinoClientError;

    fn try_from(server_url: MerinoServer) -> Result<Self, Self::Error> {
        Ok(match server_url {
            MerinoServer::Production => Url::parse("https://merino.services.mozilla.com").unwrap(),
            MerinoServer::Stage => {
                Url::parse(" https://stage.merino.nonprod.cloudops.mozgcp.net").unwrap()
            }
            MerinoServer::Custom { url } => {
                Url::parse(&url).map_err(|err| MerinoClientError::BadUrl {
                    reason: err.to_string(),
                })?
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerinoClientFetchOptions {
    pub providers: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MerinoSuggestion {
    pub title: String,
    pub url: String,
    pub provider: String,
    pub is_sponsored: bool,
    pub score: f64,
    pub icon: Option<String>,
    pub request_id: String,
}

#[derive(Deserialize)]
struct SuggestResponse {
    suggestions: Vec<SuggestionResponse>,
    request_id: String,
}

#[derive(Deserialize)]
struct SuggestionResponse {
    pub title: String,
    pub url: String,
    pub provider: String,
    pub is_sponsored: bool,
    pub score: f64,
    pub icon: Option<String>,
}

struct SessionState {
    started_at: Instant,
    session_id: String,
    sequence_number: i64,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionState {
    fn new() -> Self {
        let mut session_id_bytes = vec![0u8; 16];
        rand::fill(&mut session_id_bytes).unwrap();
        Self {
            started_at: Instant::now(),
            session_id: hex::encode(&session_id_bytes),
            sequence_number: 0,
        }
    }
}
