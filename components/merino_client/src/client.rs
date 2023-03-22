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

    /// Returns the session ID and sequence number to include in the next
    /// request to Merino.
    fn current_session_params(&self) -> (String, i64) {
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
        let (session_id, sequence_number) = self.current_session_params();

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
            status: err.status(),
        })?;

        Ok(response
            .suggestions
            .into_iter()
            .map(MerinoSuggestion::from)
            .collect())
    }

    pub fn for_server(server: MerinoServer) -> MerinoClientResult<Self> {
        Ok(Self {
            base_url: server.try_into()?,
            session_duration: Duration::from_secs(5),
            client_variants: vec![],
            default_providers: vec![],
            session_state: Default::default(),
        })
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
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Temperature {
    pub c: f64,
    pub f: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MerinoWeatherCurrentConditions {
    pub url: String,
    pub summary: String,
    pub icon_id: i64,
    pub temperature: Temperature,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MerinoWeatherForecast {
    pub url: String,
    pub summary: String,
    pub high: Temperature,
    pub low: Temperature,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MerinoSuggestion {
    Accuweather {
        details: MerinoSuggestionDetails,
        city_name: String,
        current_conditions: MerinoWeatherCurrentConditions,
        forecast: MerinoWeatherForecast,
    },
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
            SuggestResponseSuggestion::Known(
                SuggestResponseKnownProviderSuggestion::Accuweather {
                    details,
                    city_name,
                    current_conditions,
                    forecast,
                },
            ) => MerinoSuggestion::Accuweather {
                details,
                city_name,
                current_conditions,
                forecast,
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
    #[serde(rename = "accuweather")]
    Accuweather {
        #[serde(flatten)]
        details: MerinoSuggestionDetails,
        city_name: String,
        current_conditions: MerinoWeatherCurrentConditions,
        forecast: MerinoWeatherForecast,
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

    #[test]
    fn test_merino_server_returns_expected_values() -> MerinoClientResult<()> {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("GET", "/api/v1/suggest")
            .match_query(Matcher::UrlEncoded("q".into(), "test".into()))
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                    "suggestions": []
                }"#,
            )
            .create();

        let client = MerinoClient::for_server(MerinoServer::Custom {
            url: mockito::server_url(),
        })?;
        let suggestions = client.fetch("test", None)?;
        m.expect(1).assert();

        assert_eq!(suggestions.len(), 0);
        assert_eq!(client.session_duration, Duration::from_secs(5));
        Ok(())
    }

    #[test]
    fn fetch_top_pick_provider_suggestion() -> MerinoClientResult<()> {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("GET", "/api/v1/suggest")
            .match_query(Matcher::UrlEncoded("q".into(), "wiki".into()))
            .match_query(Matcher::UrlEncoded("providers".into(), "top_picks".into()))
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                  "suggestions": [
                    {
                      "title": "Wikipedia",
                      "url": "https://www.wikipedia.org/",
                      "provider": "top_picks",
                      "is_sponsored": false,
                      "score": 0.25,
                      "icon": "https://www.wikipedia.org/static/apple-touch/wikipedia.png",
                      "block_id": 0,
                      "is_top_pick": true
                    }
                  ],
                  "request_id": "e5ae54e03d1d418595ad647ae764ad19",
                  "client_variants": [],
                  "server_variants": []
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
        let suggestions = client.fetch(
            "wiki",
            Some(MerinoClientFetchOptions {
                providers: Some(vec!["top_picks".to_string()]),
            }),
        )?;
        m.expect(1).assert();

        assert_eq!(suggestions.len(), 1);
        match &suggestions[0] {
            MerinoSuggestion::TopPicks {
                details,
                block_id,
                is_top_pick,
            } => {
                assert_eq!(
                    details,
                    &MerinoSuggestionDetails {
                        title: "Wikipedia".into(),
                        url: "https://www.wikipedia.org/".into(),
                        is_sponsored: false,
                        score: 0.25,
                        icon: Some(
                            "https://www.wikipedia.org/static/apple-touch/wikipedia.png".into()
                        ),
                    }
                );
                assert_eq!(*is_top_pick, true);
                assert_eq!(*block_id, 0);
            }
            _ => assert!(false, "Wanted other suggestion; got {:?}", suggestions[0]),
        };

        Ok(())
    }

    #[test]
    fn fetch_accuweather_provider_suggestion() -> MerinoClientResult<()> {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("GET", "/api/v1/suggest")
            .match_query(Matcher::UrlEncoded("q".into(), "weather".into()))
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                   "suggestions":[
                      {
                         "title":"Weather for Milton",
                         "url":"http://www.accuweather.com/en/us/milton-wa/98354/current-weather/41512_pc?lang=en-us",
                         "provider":"accuweather",
                         "is_sponsored":false,
                         "score":0.3,
                         "icon": null,
                         "city_name":"Milton",
                         "current_conditions":{
                            "url":"http://www.accuweather.com/en/us/milton-wa/98354/current-weather/41512_pc?lang=en-us",
                            "summary":"Mostly sunny",
                            "icon_id":2,
                            "temperature":{
                               "c":-3,
                               "f":27
                            }
                         },
                         "forecast":{
                            "url":"http://www.accuweather.com/en/us/milton-wa/98354/daily-weather-forecast/41512_pc?lang=en-us",
                            "summary":"Snow tomorrow evening accumulating 1-2 inches, then changing to ice and continuing into Friday morning",
                            "high":{
                               "c":-2,
                               "f":29
                            },
                            "low":{
                               "c":-8,
                               "f":18
                            }
                         }
                      }
                   ],
                   "request_id":"0b1c8d7692b04b0ea42333c65bf65705",
                   "client_variants":[],
                   "server_variants":[]
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
        let suggestions = client.fetch("weather", None)?;
        m.expect(1).assert();

        assert_eq!(suggestions.len(), 1);
        match &suggestions[0] {
            MerinoSuggestion::Accuweather {
                details,
                city_name,
                current_conditions,
                forecast,
            } => {
                assert_eq!(
                    details,
                    &MerinoSuggestionDetails {
                        title: "Weather for Milton".into(),
                        url: "http://www.accuweather.com/en/us/milton-wa/98354/current-weather/41512_pc?lang=en-us".into(),
                        is_sponsored: false,
                        score: 0.3,
                        icon: None
                    }
                );
                assert_eq!(*city_name, "Milton");
            }
            _ => assert!(false, "Wanted other suggestion; got {:?}", suggestions[0]),
        };

        Ok(())
    }

    #[test]
    fn fetch_suggestion_with_client_variants() -> MerinoClientResult<()> {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("GET", "/api/v1/suggest")
            .match_query(Matcher::UrlEncoded("q".into(), "test".into()))
            .match_query(Matcher::UrlEncoded("client_variants".into(), "foo".into()))
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                  "suggestions": [
                    {
                        "title": "Test suggestion",
                        "url": "https://example.com",
                        "is_sponsored": false,
                        "score": 0.1,
                        "provider": "fancy_future_provider",
                        "some_field": 123
                    }
                  ],
                  "request_id": "e5ae54e03d1d418595ad647ae764ad19",
                  "client_variants": ["foo"],
                  "server_variants": []
                }"#,
            )
            .create();

        let client = MerinoClient::new(MerinoClientSettings {
            server: MerinoServer::Custom {
                url: mockito::server_url(),
            },
            session_duration_ms: 5000,
            client_variants: vec!["foo".to_string()],
            default_providers: vec![],
        })?;
        let suggestions = client.fetch("test", None)?;
        m.expect(1).assert();

        assert_eq!(suggestions.len(), 1);
        match &suggestions[0] {
            MerinoSuggestion::Other {
                details, provider, ..
            } => {
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
    #[test]
    fn fetch_suggestion_with_bad_response() -> MerinoClientResult<()> {
        viaduct_reqwest::use_reqwest_backend();
        let _m = mock("GET", "/api/v1/suggest")
            .match_query(Matcher::UrlEncoded("q".into(), "test".into()))
            .with_status(500)
            .create();

        let client = MerinoClient::new(MerinoClientSettings {
            server: MerinoServer::Custom {
                url: mockito::server_url(),
            },
            session_duration_ms: 5000,
            client_variants: vec![],
            default_providers: vec![],
        })?;
        let suggestions = client.fetch("test", None);

        match suggestions {
            Ok(suggestions) => assert!(
                false,
                "No suggestions expected, should return 500 but got {:?}",
                suggestions
            ),
            Err(MerinoClientError::FetchFailed { status, .. }) => {
                assert_eq!(status, Some(500));
            }
            Err(e) => {
                assert!(false, "Expected MerinoClientError but received {:?}", e);
            }
        }

        Ok(())
    }
}
