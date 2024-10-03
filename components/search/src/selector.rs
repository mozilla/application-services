/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::filter::filter_engine_configuration;
use crate::{
    error::Error, JSONSearchConfiguration, RefinedSearchConfig, SearchApiResult,
    SearchUserEnvironment,
};
use error_support::handle_error;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct SearchEngineSelectorInner {
    configuration: Option<JSONSearchConfiguration>,
}

/// SearchEngineSelector parses the JSON configuration for
/// search engines and returns the applicable engines depending
/// on their region + locale.
#[derive(Default, uniffi::Object)]
pub struct SearchEngineSelector(Mutex<SearchEngineSelectorInner>);

#[uniffi::export]
impl SearchEngineSelector {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self(Mutex::default())
    }

    /// Sets the search configuration from the given string. If the configuration
    /// string is unchanged since the last update, the cached configuration is
    /// reused to avoid unnecessary reprocessing. This helps optimize performance,
    /// particularly during test runs where the same configuration may be used
    /// repeatedly.
    #[handle_error(Error)]
    pub fn set_search_config(self: Arc<Self>, configuration: String) -> SearchApiResult<()> {
        if configuration.is_empty() {
            return Err(Error::SearchConfigNotSpecified);
        }
        self.0.lock().configuration = serde_json::from_str(&configuration)?;
        Ok(())
    }

    /// Clears the search configuration from memory if it is known that it is
    /// not required for a time, e.g. if the configuration will only be re-filtered
    /// after an app/environment update.
    pub fn clear_search_config(self: Arc<Self>) {}

    /// Filters the search configuration with the user's given environment,
    /// and returns the set of engines and parameters that should be presented
    /// to the user.
    #[handle_error(Error)]
    pub fn filter_engine_configuration(
        self: Arc<Self>,
        user_environment: SearchUserEnvironment,
    ) -> SearchApiResult<RefinedSearchConfig> {
        let data = match &self.0.lock().configuration {
            None => return Err(Error::SearchConfigNotSpecified),
            Some(configuration) => configuration.data.clone(),
        };
        filter_engine_configuration(user_environment, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use serde_json::json;

    #[test]
    fn test_set_config_should_allow_basic_config() {
        let selector = Arc::new(SearchEngineSelector::new());

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                {
                  "recordType": "engine",
                  "identifier": "test",
                  "base": {
                    "name": "Test",
                    "classification": "general",
                    "urls": {
                      "search": {
                        "base": "https://example.com",
                        "method": "GET"
                      }
                    }
                  }
                },
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test"
                }
              ]
            })
            .to_string(),
        );
        assert!(
            config_result.is_ok(),
            "Should not have errored: `{config_result:?}`"
        );
    }

    #[test]
    fn test_set_config_should_allow_extra_fields() {
        let selector = Arc::new(SearchEngineSelector::new());

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                {
                  "recordType": "engine",
                  "identifier": "test",
                  "base": {
                    "name": "Test",
                    "classification": "general",
                    "urls": {
                      "search": {
                        "base": "https://example.com",
                        "method": "GET",
                        "extraField1": true
                      }
                    },
                    "extraField2": "123"
                  },
                  "extraField3": ["foo"]
                },
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test",
                  "extraField4": {
                    "subField1": true
                  }
                }
              ]
            })
            .to_string(),
        );
        assert!(
            config_result.is_ok(),
            "Should not have errored: `{config_result:?}`"
        );
    }

    #[test]
    fn test_set_config_should_ignore_unknown_record_types() {
        let selector = Arc::new(SearchEngineSelector::new());

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                {
                  "recordType": "engine",
                  "identifier": "test",
                  "base": {
                    "name": "Test",
                    "classification": "general",
                    "urls": {
                      "search": {
                        "base": "https://example.com",
                        "method": "GET"
                      }
                    }
                  }
                },
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test"
                },
                {
                  "recordType": "unknown"
                }
              ]
            })
            .to_string(),
        );
        assert!(
            config_result.is_ok(),
            "Should not have errored: `{config_result:?}`"
        );
    }

    #[test]
    fn test_filter_engine_configuration_throws_without_config() {
        let selector = Arc::new(SearchEngineSelector::new());

        let result = selector.filter_engine_configuration(SearchUserEnvironment {
            locale: "fi".into(),
            region: "FR".into(),
            update_channel: SearchUpdateChannel::Default,
            distribution_id: String::new(),
            experiment: String::new(),
            app_name: SearchApplicationName::Firefox,
            version: String::new(),
        });

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Search configuration not specified"))
    }

    #[test]
    fn test_filter_engine_configuration_returns_basic_engines() {
        let selector = Arc::new(SearchEngineSelector::new());

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                {
                  "recordType": "engine",
                  "identifier": "test1",
                  "base": {
                    "name": "Test 1",
                    "classification": "general",
                    "urls": {
                      "search": {
                        "base": "https://example.com/1",
                        "method": "GET",
                        "searchTermParamName": "q"
                      }
                    }
                  }
                },
                {
                  "recordType": "engine",
                  "identifier": "test2",
                  "base": {
                    "name": "Test 2",
                    "classification": "general",
                    "urls": {
                      "search": {
                        "base": "https://example.com/2",
                        "method": "GET",
                        "searchTermParamName": "search"
                      }
                    }
                  }
                },
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test1",
                  "globalDefaultPrivate": "test2"
                }
              ]
            })
            .to_string(),
        );
        assert!(
            config_result.is_ok(),
            "Should not have errored: `{config_result:?}`"
        );

        let result = selector.filter_engine_configuration(SearchUserEnvironment {
            locale: "fi".into(),
            region: "FR".into(),
            update_channel: SearchUpdateChannel::Default,
            distribution_id: String::new(),
            experiment: String::new(),
            app_name: SearchApplicationName::Firefox,
            version: String::new(),
        });

        assert!(result.is_ok(), "Should not have errored: `{result:?}`");
        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(
                    SearchEngineDefinition {
                        aliases: Vec::new(),
                        charset: "UTF-8".to_string(),
                        classification: SearchEngineClassification::General,
                        identifier: "test1".to_string(),
                        name: "Test 1".to_string(),
                        order_hint: None,
                        partner_code: String::new(),
                        telemetry_suffix: None,
                        urls: SearchEngineUrls {
                            search: SearchEngineUrl {
                                base: "https://example.com/1".to_string(),
                                method: "GET".to_string(),
                                params: Vec::new(),
                                search_term_param_name: Some("q".to_string())
                            },
                            suggestions: None,
                            trending: None
                        }
                    },
                    SearchEngineDefinition {
                        aliases: Vec::new(),
                        charset: "UTF-8".to_string(),
                        classification: SearchEngineClassification::General,
                        identifier: "test2".to_string(),
                        name: "Test 2".to_string(),
                        order_hint: None,
                        partner_code: String::new(),
                        telemetry_suffix: None,
                        urls: SearchEngineUrls {
                            search: SearchEngineUrl {
                                base: "https://example.com/2".to_string(),
                                method: "GET".to_string(),
                                params: Vec::new(),
                                search_term_param_name: Some("search".to_string())
                            },
                            suggestions: None,
                            trending: None
                        }
                    }
                ),
                app_default_engine_id: "test1".to_string(),
                app_default_private_engine_id: Some("test2".to_string())
            }
        )
    }
}
