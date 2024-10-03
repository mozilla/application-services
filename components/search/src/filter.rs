/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    error::Error, JSONEngineBase, JSONEngineRecord, JSONEngineUrl, JSONEngineUrls,
    JSONSearchConfigurationRecords, RefinedSearchConfig, SearchEngineDefinition, SearchEngineUrl,
    SearchEngineUrls, SearchUserEnvironment,
};

impl SearchEngineUrl {
    pub(crate) fn from_engine_url(url: &JSONEngineUrl) -> Self {
        Self {
            base: url.base.clone(),
            method: match &url.method {
                None => "GET".to_string(),
                Some(method) => method.as_str().to_string(),
            },
            params: url.params.clone().unwrap_or_default(),
            search_term_param_name: url.search_term_param_name.clone(),
        }
    }
}

impl SearchEngineUrls {
    pub(crate) fn from_engine_urls(urls: &JSONEngineUrls) -> Self {
        Self {
            search: SearchEngineUrl::from_engine_url(&urls.search),
            suggestions: None,
            trending: None,
        }
    }
}

impl SearchEngineDefinition {
    pub(crate) fn from_configuration_details(
        identifier: &str,
        base: &JSONEngineBase,
    ) -> SearchEngineDefinition {
        SearchEngineDefinition {
            aliases: base.aliases.clone().unwrap_or_default(),
            charset: base.charset.clone().unwrap_or("UTF-8".to_string()),
            classification: base.classification.clone(),
            identifier: identifier.to_owned(),
            name: base.name.clone(),
            order_hint: None,
            partner_code: base.partner_code.clone().unwrap_or_default(),
            telemetry_suffix: None,
            urls: SearchEngineUrls::from_engine_urls(&base.urls),
        }
    }
}

pub(crate) fn filter_engine_configuration(
    user_environment: SearchUserEnvironment,
    configuration: &Vec<JSONSearchConfigurationRecords>,
) -> Result<RefinedSearchConfig, Error> {
    let mut engines = Vec::new();
    let mut default_engine_id: Option<String> = None;
    let mut default_private_engine_id: Option<String> = None;

    for record in configuration {
        match record {
            JSONSearchConfigurationRecords::Engine(engine) => {
                let result = extract_engine_config(&user_environment, engine);
                if let Some(result) = result {
                    engines.push(result)
                };
            }
            JSONSearchConfigurationRecords::DefaultEngines(default_engines) => {
                default_engine_id = Some(default_engines.global_default.clone());
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

fn extract_engine_config(
    _user_environment: &SearchUserEnvironment,
    record: &JSONEngineRecord,
) -> Option<SearchEngineDefinition> {
    // TODO: Variant handling.
    Some(SearchEngineDefinition::from_configuration_details(
        &record.identifier,
        &record.base,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_from_configuration_details_fallsback_to_defaults() {
        let base = JSONEngineBase {
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
        };
        let result = SearchEngineDefinition::from_configuration_details("test", &base);

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
        let base = JSONEngineBase {
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
                suggestions: None,
                trending: None,
            },
        };
        let result = SearchEngineDefinition::from_configuration_details("test", &base);

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
                    suggestions: None,
                    trending: None
                }
            }
        )
    }
}
