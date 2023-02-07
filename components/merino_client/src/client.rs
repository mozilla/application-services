/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    fmt::{self, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rc_crypto::rand;
use serde_derive::*;
use url::Url;
use viaduct::Request;

use crate::error::{InternalError, MerinoClientError};

pub type MerinoClientResult<T> = std::result::Result<T, MerinoClientError>;

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
        let mut endpoint_url = self.base_url.join("/api/v1/suggest")?;
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
        let response = (|| -> Result<SuggestResponse, InternalError> {
            Ok(request.send()?.require_success()?.json()?)
        })()
        .map_err(|err| MerinoClientError::FetchFailed {
            reason: err.to_string(),
        })?;

        Ok(response
            .suggestions
            .into_iter()
            .map(MerinoSuggestion::from)
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

    fn try_from(server_url: MerinoServer) -> MerinoClientResult<Self> {
        Ok(match server_url {
            MerinoServer::Production => Url::parse("https://merino.services.mozilla.com").unwrap(),
            MerinoServer::Stage => {
                Url::parse(" https://stage.merino.nonprod.cloudops.mozgcp.net").unwrap()
            }
            MerinoServer::Custom { url } => Url::parse(&url)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerinoClientFetchOptions {
    pub providers: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MerinoSuggestionDetails {
    pub title: String,
    pub url: String,
    pub is_sponsored: bool,
    pub score: f64,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MerinoSuggestion {
    Adm {
        details: MerinoSuggestionDetails,
        block_id: i64,
        full_keyword: String,
        advertiser: String,
        impression_url: Option<String>,
        click_url: Option<String>,
    },
    TopPicks {
        details: MerinoSuggestionDetails,
        block_id: i64,
        is_top_pick: bool,
    },
    Other {
        details: MerinoSuggestionDetails,
        provider: String,
    },
}

impl From<SuggestResponseSuggestion> for MerinoSuggestion {
    fn from(suggestion: SuggestResponseSuggestion) -> Self {
        match suggestion {
            SuggestResponseSuggestion::Known(SuggestResponseKnownProviderSuggestion::Adm {
                details,
                block_id,
                full_keyword,
                advertiser,
                impression_url,
                click_url,
            }) => MerinoSuggestion::Adm {
                details,
                block_id,
                full_keyword,
                advertiser,
                impression_url,
                click_url,
            },
            SuggestResponseSuggestion::Known(
                SuggestResponseKnownProviderSuggestion::TopPicks {
                    details,
                    block_id,
                    is_top_pick,
                },
            ) => MerinoSuggestion::TopPicks {
                details,
                block_id,
                is_top_pick,
            },
            SuggestResponseSuggestion::Unknown { details, provider } => {
                MerinoSuggestion::Other { details, provider }
            }
        }
    }
}

#[derive(Deserialize)]
struct SuggestResponse {
    suggestions: Vec<SuggestResponseSuggestion>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SuggestResponseSuggestion {
    Known(SuggestResponseKnownProviderSuggestion),
    // `#[serde(other)]` doesn't support associated data, so we can't
    // deserialize the response suggestion directly into a `MerinoSuggestion`.
    // Instead, we have an "outer", untagged `SuggestResponseSuggestion` that
    // has our unknown / other variant, and an "inner", internally tagged
    // `SuggestResponseKnownProviderSuggestion` with our known variants.
    Unknown {
        #[serde(flatten)]
        details: MerinoSuggestionDetails,
        provider: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "provider")]
enum SuggestResponseKnownProviderSuggestion {
    #[serde(rename = "adm")]
    Adm {
        #[serde(flatten)]
        details: MerinoSuggestionDetails,
        block_id: i64,
        full_keyword: String,
        advertiser: String,
        impression_url: Option<String>,
        click_url: Option<String>,
    },
    #[serde(rename = "top_picks")]
    TopPicks {
        #[serde(flatten)]
        details: MerinoSuggestionDetails,
        block_id: i64,
        is_top_pick: bool,
    },
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
        Self {
            started_at: Instant::now(),
            session_id: generate_session_id(),
            sequence_number: 0,
        }
    }
}

fn generate_session_id() -> String {
    let mut bytes = vec![0u8; 16];
    rand::fill(&mut bytes).unwrap();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut string = String::with_capacity(36);
    write!(
        &mut string,
        "{}-{}-{}-{}-{}",
        HexSequence(&bytes[0..4]),
        HexSequence(&bytes[4..6]),
        HexSequence(&bytes[6..8]),
        HexSequence(&bytes[8..10]),
        HexSequence(&bytes[10..16])
    )
    .unwrap();

    return string;
}

struct HexSequence<'a>(&'a [u8]);

impl fmt::Display for HexSequence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(self
            .0
            .iter()
            .try_for_each(|byte| f.write_fmt(format_args!("{:02x}", byte)))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{mock, Matcher};

    #[test]
    fn fetch_adm_suggestion() -> MerinoClientResult<()> {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("GET", "/api/v1/suggest")
            .match_query(Matcher::UrlEncoded("q".into(), "test".into()))
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                    "suggestions": [{
                        "title": "Test adM suggestion",
                        "url": "https://example.com",
                        "is_sponsored": false,
                        "score": 0.1,
                        "provider": "adm",
                        "block_id": 1,
                        "full_keyword": "testing",
                        "advertiser": "Test"
                    }]
                }"#,
            )
            .create();

        let client = MerinoClient::new(MerinoClientSettings {
            server: MerinoServer::Custom {
                url: mockito::server_url(),
            },
            session_duration_ms: 5000,
            client_variants: vec![],
            default_providers: vec![],
        })?;
        let suggestions = client.fetch("test", None)?;
        m.expect(1).assert();

        assert_eq!(suggestions.len(), 1);
        match &suggestions[0] {
            MerinoSuggestion::Adm {
                details,
                block_id,
                full_keyword,
                advertiser,
                ..
            } => {
                assert_eq!(
                    details,
                    &MerinoSuggestionDetails {
                        title: "Test adM suggestion".into(),
                        url: "https://example.com".into(),
                        is_sponsored: false,
                        score: 0.1,
                        icon: None,
                    }
                );
                assert_eq!(*block_id, 1);
                assert_eq!(full_keyword, "testing");
                assert_eq!(advertiser, "Test");
            }
            _ => assert!(false, "Wanted adM suggestion; got {:?}", suggestions[0]),
        };

        Ok(())
    }

    #[test]
    fn fetch_unknown_provider_suggestion() -> MerinoClientResult<()> {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("GET", "/api/v1/suggest")
            .match_query(Matcher::UrlEncoded("q".into(), "test".into()))
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                    "suggestions": [{
                        "title": "Test suggestion",
                        "url": "https://example.com",
                        "is_sponsored": false,
                        "score": 0.1,
                        "provider": "fancy_future_provider",
                        "some_field": 123
                    }]
                }"#,
            )
            .create();

        let client = MerinoClient::new(MerinoClientSettings {
            server: MerinoServer::Custom {
                url: mockito::server_url(),
            },
            session_duration_ms: 5000,
            client_variants: vec![],
            default_providers: vec![],
        })?;
        let suggestions = client.fetch("test", None)?;
        m.expect(1).assert();

        assert_eq!(suggestions.len(), 1);
        match &suggestions[0] {
            MerinoSuggestion::Other { details, provider } => {
                assert_eq!(
                    details,
                    &MerinoSuggestionDetails {
                        title: "Test suggestion".into(),
                        url: "https://example.com".into(),
                        is_sponsored: false,
                        score: 0.1,
                        icon: None,
                    }
                );
                assert_eq!(provider, "fancy_future_provider");
            }
            _ => assert!(false, "Wanted other suggestion; got {:?}", suggestions[0]),
        };

        Ok(())
    }
}
