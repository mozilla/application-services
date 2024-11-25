/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines the functions for managing the filtering of the configuration.

use crate::environment_matching::matches_user_environment;
use crate::{
    error::Error, JSONEngineBase, JSONEngineRecord, JSONEngineUrl, JSONEngineUrls,
    JSONSearchConfigurationRecords, RefinedSearchConfig, SearchEngineDefinition, SearchEngineUrl,
    SearchEngineUrls, SearchUserEnvironment,
};

impl From<JSONEngineUrl> for SearchEngineUrl {
    fn from(url: JSONEngineUrl) -> Self {
        Self {
            base: url.base,
            method: url.method.unwrap_or_default().as_str().to_string(),
            params: url.params.unwrap_or_default(),
            search_term_param_name: url.search_term_param_name,
        }
    }
}

impl From<JSONEngineUrls> for SearchEngineUrls {
    fn from(urls: JSONEngineUrls) -> Self {
        Self {
            search: urls.search.into(),
            suggestions: urls.suggestions.map(|suggestions| suggestions.into()),
            trending: urls.trending.map(|trending| trending.into()),
        }
    }
}

impl SearchEngineDefinition {
    pub(crate) fn from_configuration_details(
        identifier: &str,
        base: JSONEngineBase,
    ) -> SearchEngineDefinition {
        SearchEngineDefinition {
            aliases: base.aliases.unwrap_or_default(),
            charset: base.charset.unwrap_or_else(|| "UTF-8".to_string()),
            classification: base.classification,
            identifier: identifier.to_string(),
            name: base.name,
            order_hint: None,
            partner_code: base.partner_code.unwrap_or_default(),
            telemetry_suffix: None,
            urls: base.urls.into(),
        }
    }
}

pub(crate) fn filter_engine_configuration(
    user_environment: SearchUserEnvironment,
    configuration: Vec<JSONSearchConfigurationRecords>,
) -> Result<RefinedSearchConfig, Error> {
    let mut engines = Vec::new();
    let mut default_engine_id: Option<String> = None;
    let mut default_private_engine_id: Option<String> = None;

    let mut user_environment = user_environment.clone();
    user_environment.locale = user_environment.locale.to_lowercase();
    user_environment.region = user_environment.region.to_lowercase();
    user_environment.version = user_environment.version.to_lowercase();

    for record in configuration {
        match record {
            JSONSearchConfigurationRecords::Engine(engine) => {
                let result = maybe_extract_engine_config(&user_environment, engine);
                engines.extend(result);
            }
            JSONSearchConfigurationRecords::DefaultEngines(default_engines) => {
                default_engine_id = Some(default_engines.global_default);
                default_private_engine_id.clone_from(&default_engines.global_default_private);
            }
            JSONSearchConfigurationRecords::EngineOrders(_engine_orders) => {
                // TODO: Implementation.
            }
            JSONSearchConfigurationRecords::Unknown => {
                // Prevents panics if a new record type is added in future.
            }
        }
    }

    Ok(RefinedSearchConfig {
        engines,
        app_default_engine_id: default_engine_id.unwrap(),
        app_default_private_engine_id: default_private_engine_id,
    })
}

fn maybe_extract_engine_config(
    user_environment: &SearchUserEnvironment,
    record: Box<JSONEngineRecord>,
) -> Option<SearchEngineDefinition> {
    let JSONEngineRecord {
        identifier,
        variants,
        base,
    } = *record;
    let matching_variant = variants
        .into_iter()
        .rev()
        .find(|r| matches_user_environment(&r.environment, user_environment));

    matching_variant
        .map(|_variant| SearchEngineDefinition::from_configuration_details(&identifier, base))
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::*;

    #[test]
    fn test_from_configuration_details_fallsback_to_defaults() {
        let result = SearchEngineDefinition::from_configuration_details(
            "test",
            JSONEngineBase {
                aliases: None,
                charset: None,
                classification: SearchEngineClassification::General,
                name: "Test".to_string(),
                partner_code: None,
                urls: JSONEngineUrls {
                    search: JSONEngineUrl {
                        base: "https://example.com".to_string(),
                        method: None,
                        params: None,
                        search_term_param_name: None,
                    },
                    suggestions: None,
                    trending: None,
                },
            },
        );

        assert_eq!(
            result,
            SearchEngineDefinition {
                aliases: Vec::new(),
                charset: "UTF-8".to_string(),
                classification: SearchEngineClassification::General,
                identifier: "test".to_string(),
                partner_code: String::new(),
                name: "Test".to_string(),
                order_hint: None,
                telemetry_suffix: None,
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com".to_string(),
                        method: "GET".to_string(),
                        params: Vec::new(),
                        search_term_param_name: None,
                    },
                    suggestions: None,
                    trending: None
                }
            }
        )
    }

    #[test]
    fn test_from_configuration_details_uses_values() {
        let result = SearchEngineDefinition::from_configuration_details(
            "test",
            JSONEngineBase {
                aliases: Some(vec!["foo".to_string(), "bar".to_string()]),
                charset: Some("ISO-8859-15".to_string()),
                classification: SearchEngineClassification::Unknown,
                name: "Test".to_string(),
                partner_code: Some("firefox".to_string()),
                urls: JSONEngineUrls {
                    search: JSONEngineUrl {
                        base: "https://example.com".to_string(),
                        method: Some(crate::JSONEngineMethod::Post),
                        params: Some(vec![SearchUrlParam {
                            name: "param".to_string(),
                            value: Some("test param".to_string()),
                            experiment_config: None,
                        }]),
                        search_term_param_name: Some("baz".to_string()),
                    },
                    suggestions: Some(JSONEngineUrl {
                        base: "https://example.com/suggestions".to_string(),
                        method: Some(crate::JSONEngineMethod::Get),
                        params: Some(vec![SearchUrlParam {
                            name: "suggest-name".to_string(),
                            value: None,
                            experiment_config: Some("suggest-experiment-value".to_string()),
                        }]),
                        search_term_param_name: Some("suggest".to_string()),
                    }),
                    trending: Some(JSONEngineUrl {
                        base: "https://example.com/trending".to_string(),
                        method: Some(crate::JSONEngineMethod::Get),
                        params: Some(vec![SearchUrlParam {
                            name: "trend-name".to_string(),
                            value: Some("trend-value".to_string()),
                            experiment_config: None,
                        }]),
                        search_term_param_name: None,
                    }),
                },
            },
        );

        assert_eq!(
            result,
            SearchEngineDefinition {
                aliases: vec!["foo".to_string(), "bar".to_string()],
                charset: "ISO-8859-15".to_string(),
                classification: SearchEngineClassification::Unknown,
                identifier: "test".to_string(),
                partner_code: "firefox".to_string(),
                name: "Test".to_string(),
                order_hint: None,
                telemetry_suffix: None,
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com".to_string(),
                        method: "POST".to_string(),
                        params: vec![SearchUrlParam {
                            name: "param".to_string(),
                            value: Some("test param".to_string()),
                            experiment_config: None,
                        }],
                        search_term_param_name: Some("baz".to_string()),
                    },
                    suggestions: Some(SearchEngineUrl {
                        base: "https://example.com/suggestions".to_string(),
                        method: "GET".to_string(),
                        params: vec![SearchUrlParam {
                            name: "suggest-name".to_string(),
                            value: None,
                            experiment_config: Some("suggest-experiment-value".to_string()),
                        }],
                        search_term_param_name: Some("suggest".to_string()),
                    }),
                    trending: Some(SearchEngineUrl {
                        base: "https://example.com/trending".to_string(),
                        method: "GET".to_string(),
                        params: vec![SearchUrlParam {
                            name: "trend-name".to_string(),
                            value: Some("trend-value".to_string()),
                            experiment_config: None,
                        }],
                        search_term_param_name: None,
                    })
                }
            }
        )
    }
}
