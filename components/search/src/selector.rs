/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines the main `SearchEngineSelector`.

use crate::configuration_overrides_types::JSONOverridesRecord;
use crate::configuration_overrides_types::JSONSearchConfigurationOverrides;
use crate::filter::filter_engine_configuration_impl;
use crate::{
    error::Error, JSONSearchConfiguration, RefinedSearchConfig, SearchApiResult,
    SearchUserEnvironment,
};
use error_support::handle_error;
use parking_lot::Mutex;
use remote_settings::{RemoteSettingsClient, RemoteSettingsService};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct SearchEngineSelectorInner {
    configuration: Option<JSONSearchConfiguration>,
    configuration_overrides: Option<JSONSearchConfigurationOverrides>,
    search_config_client: Option<Arc<RemoteSettingsClient>>,
    search_config_overrides_client: Option<Arc<RemoteSettingsClient>>,
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

    /// Sets the RemoteSettingsService to use. The selector will create the
    /// relevant remote settings client(s) from the service.
    ///
    /// # Params:
    ///   - `service`: The remote settings service instance for the application.
    ///   - `options`: The remote settings options to be passed to the client(s).
    ///   - `apply_engine_overrides`: Whether or not to apply overrides from
    ///     `search-config-v2-overrides` to the selected engines. Should be false unless the
    ///     application supports the click URL feature.
    pub fn use_remote_settings_server(
        self: Arc<Self>,
        service: &Arc<RemoteSettingsService>,
        apply_engine_overrides: bool,
    ) {
        let mut inner = self.0.lock();
        inner.search_config_client = Some(service.make_client("search-config-v2".to_string()));

        if apply_engine_overrides {
            inner.search_config_overrides_client =
                Some(service.make_client("search-config-overrides-v2".to_string()));
        }
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

    #[handle_error(Error)]
    pub fn set_config_overrides(self: Arc<Self>, overrides: String) -> SearchApiResult<()> {
        if overrides.is_empty() {
            return Err(Error::SearchConfigOverridesNotSpecified);
        }
        self.0.lock().configuration_overrides = serde_json::from_str(&overrides)?;
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
        let inner = self.0.lock();
        if let Some(client) = &inner.search_config_client {
            // Remote settings ships dumps of the collections, so it is highly
            // unlikely that we'll ever hit the case where we have no records.
            // However, just in case of an issue that does causes us to receive
            // no records, we will raise an error so that the application can
            // handle or record it appropriately.
            let records = client.get_records(false);

            if let Some(records) = records {
                if records.is_empty() {
                    return Err(Error::SearchConfigNoRecords);
                }

                if let Some(overrides_client) = &inner.search_config_overrides_client {
                    let overrides_records = overrides_client.get_records(false);

                    if let Some(overrides_records) = overrides_records {
                        if overrides_records.is_empty() {
                            return filter_engine_configuration_impl(
                                user_environment,
                                &records,
                                None,
                            );
                        }
                        // TODO: Bug 1947241 - Find a way to avoid having to serialise the records
                        // back to strings and then deserialise them into the records that we want.
                        let stringified = serde_json::to_string(&overrides_records)?;
                        let json_overrides: Vec<JSONOverridesRecord> =
                            serde_json::from_str(&stringified)?;

                        return filter_engine_configuration_impl(
                            user_environment,
                            &records,
                            Some(json_overrides),
                        );
                    } else {
                        return Err(Error::SearchConfigOverridesNoRecords);
                    }
                }

                return filter_engine_configuration_impl(user_environment, &records, None);
            } else {
                return Err(Error::SearchConfigNoRecords);
            }
        }
        let config = match &inner.configuration {
            None => return Err(Error::SearchConfigNotSpecified),
            Some(configuration) => configuration.data.clone(),
        };

        let config_overrides = match &inner.configuration_overrides {
            None => return Err(Error::SearchConfigOverridesNotSpecified),
            Some(overrides) => overrides.data.clone(),
        };
        return filter_engine_configuration_impl(user_environment, &config, Some(config_overrides));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{EngineRecord, ExpectedEngine, SubVariant, Variant};
    use crate::{test_helpers, types::*, SearchApiError};
    use mockito::mock;
    use remote_settings::{RemoteSettingsConfig2, RemoteSettingsContext, RemoteSettingsServer};
    use serde_json::json;

    #[test]
    fn test_set_config_should_allow_basic_config() {
        let selector = Arc::new(SearchEngineSelector::new());

        let config = json!({
            "data": [
                EngineRecord::full("test1", "Test 1").build(),
                {
                    "recordType": "defaultEngines",
                    "globalDefault": "test"
                }
            ]
        });

        let config_result = Arc::clone(&selector).set_search_config(config.to_string());
        config_result.expect("Should have set the configuration successfully");
    }

    #[test]
    fn test_set_config_should_allow_extra_fields() {
        let selector = Arc::new(SearchEngineSelector::new());

        let mut engine = EngineRecord::minimal("test", "Test").build();
        engine["base"]["urls"]["search"]["extraField1"] = json!(true);
        engine["base"]["extraField2"] = json!("123");
        engine["extraField3"] = json!(["foo"]);

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                engine,
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
        config_result.expect("Should have set the configuration successfully with extra fields");
    }

    #[test]
    fn test_set_config_should_ignore_unknown_record_types() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config = json!({
            "data": [
                EngineRecord::full("test1", "Test 1").build(),
                {
                    "recordType": "defaultEngines",
                    "globalDefault": "test"
                },
                {
                  "recordType": "unknown"
                }
            ]
        });
        let config_result = Arc::clone(&selector).set_search_config(config.to_string());

        config_result
            .expect("Should have set the configuration successfully with unknown record types.");
    }

    #[test]
    fn test_filter_engine_configuration_throws_without_config() {
        let selector = Arc::new(SearchEngineSelector::new());

        let result = selector.filter_engine_configuration(SearchUserEnvironment {
            ..Default::default()
        });

        assert!(
            result.is_err(),
            "Should throw an error when a configuration has not been specified before filtering"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Search configuration not specified"))
    }

    #[test]
    fn test_filter_engine_configuration_throws_without_config_overrides() {
        let selector = Arc::new(SearchEngineSelector::new());
        let _ = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::full("test1", "Test 1").build(),
            ]
            })
            .to_string(),
        );

        let result = selector.filter_engine_configuration(SearchUserEnvironment {
            ..Default::default()
        });

        assert!(
            result.is_err(),
            "Should throw an error when a configuration overrides has not been specified before filtering"
        );

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Search configuration overrides not specified"))
    }

    #[test]
    fn test_filter_engine_configuration_returns_basic_engines() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({ "data": [test_helpers::overrides_engine()] }).to_string(),
        );

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::full("test1", "Test 1").build(),
                EngineRecord::minimal("test2", "Test 2").build(),
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test1",
                  "globalDefaultPrivate": "test2"
                }
              ]
            })
            .to_string(),
        );
        config_result.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        let result = selector.filter_engine_configuration(SearchUserEnvironment {
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );
        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(
                    ExpectedEngine::full("test1", "Test 1").build(),
                    ExpectedEngine::minimal("test2", "Test 2").build(),
                ),
                app_default_engine_id: Some("test1".to_string()),
                app_private_default_engine_id: Some("test2".to_string())
            }
        )
    }

    #[test]
    fn test_filter_engine_configuration_handles_basic_variants() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({ "data": [test_helpers::overrides_engine()] }).to_string(),
        );

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::full("test1", "Test 1")
                .add_variant(
                    Variant::new()
                        .regions(&["FR"])
                        .urls(json!({
                            "search": {
                                "method": "POST",
                                "params": [{
                                    "name": "mission",
                                    "value": "ongoing"
                                }]
                            }
                        }))
                )
                .build(),
                EngineRecord::minimal("test2", "Test 2")
                .add_variant(
                    Variant::new()
                        .optional(true)
                        .partner_code("ship")
                        .telemetry_suffix("E")
                )
                .build(),
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test1",
                  "globalDefaultPrivate": "test2"
                }
              ]
            })
            .to_string(),
        );
        config_result.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        let result = selector.filter_engine_configuration(SearchUserEnvironment {
            region: "FR".into(),
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        let expected_1 = ExpectedEngine::full("test1", "Test 1")
            .search_method("POST")
            .search_params(vec![SearchUrlParam {
                name: "mission".to_string(),
                value: Some("ongoing".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }])
            .build();

        let expected_2 = ExpectedEngine::minimal("test2", "Test 2")
            .optional(true)
            .partner_code("ship")
            .telemetry_suffix("E")
            .build();

        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(expected_1, expected_2),
                app_default_engine_id: Some("test1".to_string()),
                app_private_default_engine_id: Some("test2".to_string())
            }
        );
    }

    #[test]
    fn test_filter_engine_configuration_handles_basic_subvariants() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({ "data": [test_helpers::overrides_engine()] }).to_string(),
        );

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::full("test1", "Test 1")
                  .add_variant(
                    Variant::new()
                      .regions(&["FR"])
                      .add_subvariant(
                        SubVariant::new()
                          .locales(&["fr"])
                          .partner_code("fr-partner-code")
                          .telemetry_suffix("fr-telemetry-suffix"),
                      )
                      .add_subvariant(
                        SubVariant::new()
                          .locales(&["en-CA"])
                          .urls(json!({
                            "search": {
                              "method": "GET",
                              "params": [{
                                "name": "en-ca-param-name",
                                "enterpriseValue": "en-ca-param-value"
                              }]
                            }
                          })),
                      )
                  )
                  .build(),
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test1"
                },
                {
                  "recordType": "availableLocales",
                  "locales": ["en-CA", "fr"]
                }
              ]
            })
            .to_string(),
        );
        config_result.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        let mut result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            region: "FR".into(),
            locale: "fr".into(),
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        let expected_1 = ExpectedEngine::full("test1", "Test 1")
            .partner_code("fr-partner-code")
            .telemetry_suffix("fr-telemetry-suffix")
            .build();

        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(expected_1),
                app_default_engine_id: Some("test1".to_string()),
                app_private_default_engine_id: None
            },
            "Should have correctly matched and merged the fr locale sub-variant."
        );

        result = selector.filter_engine_configuration(SearchUserEnvironment {
            region: "FR".into(),
            locale: "en-CA".into(),
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        let expected_2 = ExpectedEngine::full("test1", "Test 1")
            .search_params(vec![SearchUrlParam {
                name: "en-ca-param-name".to_string(),
                value: None,
                enterprise_value: Some("en-ca-param-value".to_string()),
                experiment_config: None,
            }])
            .build();

        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(expected_2),
                app_default_engine_id: Some("test1".to_string()),
                app_private_default_engine_id: None
            },
            "Should have correctly matched and merged the en-CA locale sub-variant."
        );
    }

    #[test]
    fn test_filter_engine_configuration_handles_environments() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({ "data": [test_helpers::overrides_engine()] }).to_string(),
        );

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::full("test1", "Test 1").build(),
                EngineRecord::full("test2", "Test 2")
                .override_variants(
                    Variant::new()
                      .applications(&["firefox-android", "focus-ios"])
                )
                .build(),
                EngineRecord::full("test3", "Test 3")
                .override_variants(
                    Variant::new()
                      .distributions(&["starship"])
                )
                .build(),
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test1",
                }
              ]
            })
            .to_string(),
        );
        config_result.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        let mut result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: String::new(),
            app_name: SearchApplicationName::Firefox,
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(ExpectedEngine::full("test1", "Test 1").build()),
                app_default_engine_id: Some("test1".to_string()),
                app_private_default_engine_id: None
            }, "Should have selected test1 for all matching locales, as the environments do not match for the other two"
        );

        result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: String::new(),
            app_name: SearchApplicationName::FocusIos,
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        let expected_1 = ExpectedEngine::full("test1", "Test 1").build();
        let expected_2 = ExpectedEngine::full("test2", "Test 2").build();
        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(expected_1, expected_2),
                app_default_engine_id: Some("test1".to_string()),
                app_private_default_engine_id: None
            },
            "Should have selected test1 for all matching locales and test2 for matching Focus IOS"
        );

        result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: "starship".to_string(),
            app_name: SearchApplicationName::Firefox,
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        let expected_1 = ExpectedEngine::full("test1", "Test 1").build();
        let expected_3 = ExpectedEngine::full("test3", "Test 3").build();
        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec!(expected_1, expected_3),
                app_default_engine_id: Some("test1".to_string()),
                app_private_default_engine_id: None
            }, "Should have selected test1 for all matching locales and test3 for matching the distribution id"
        );
    }

    #[test]
    fn test_set_config_should_handle_default_engines() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({ "data": [test_helpers::overrides_engine()] }).to_string(),
        );

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::minimal("test", "Test").build(),
                EngineRecord::minimal("distro-default", "Distribution Default").build(),
                EngineRecord::minimal("private-default-FR", "Private default FR").build(),
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "test",
                  "specificDefaults": [{
                    "environment": {
                      "distributions": ["test-distro"],
                    },
                    "default": "distro-default"
                  }, {
                    "environment": {
                      "regions": ["fr"]
                    },
                    "defaultPrivate": "private-default-FR"
                  }]
                }
              ]
            })
            .to_string(),
        );
        config_result.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: "test-distro".to_string(),
            ..Default::default()
        });
        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec![
                    ExpectedEngine::minimal("distro-default", "Distribution Default").build(),
                    ExpectedEngine::minimal("private-default-FR", "Private default FR").build(),
                    ExpectedEngine::minimal("test", "Test").build(),
                ],
                app_default_engine_id: Some("distro-default".to_string()),
                app_private_default_engine_id: None
            },
            "Should have selected the distro-default engine for the matching specific default"
        );

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            region: "fr".into(),
            distribution_id: String::new(),
            ..Default::default()
        });
        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec![
                    ExpectedEngine::minimal("test", "Test").build(),
                    ExpectedEngine::minimal("private-default-FR", "Private default FR").build(),
                    ExpectedEngine::minimal("distro-default", "Distribution Default").build(),
                ],
                app_default_engine_id: Some("test".to_string()),
                app_private_default_engine_id: Some("private-default-FR".to_string())
            },
            "Should have selected the private default engine for the matching specific default"
        );
    }

    #[test]
    fn test_filter_engine_orders() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({ "data": [test_helpers::overrides_engine()] }).to_string(),
        );

        let engine_order_config = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::minimal("after-defaults", "after-defaults").build(),
                EngineRecord::minimal("b-engine", "first alphabetical").build(),
                EngineRecord::minimal("a-engine", "last alphabetical").build(),
                EngineRecord::minimal("default-engine", "default-engine").build(),
                EngineRecord::minimal("default-private-engine", "default-privite-engine").build(),
                {
                  "recordType": "defaultEngines",
                  "globalDefault": "default-engine",
                  "globalDefaultPrivate": "default-private-engine",
                },
                {
                  "recordType": "engineOrders",
                  "orders": [
                    {
                      "environment": {
                        "locales": ["en-CA"],
                        "regions": ["CA"],
                      },
                      "order": ["after-defaults"],
                    },
                  ],
                },
                {
                  "recordType": "availableLocales",
                  "locales": ["en-CA", "fr"]
                }
              ]
            })
            .to_string(),
        );
        engine_order_config.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        fn assert_actual_engines_equals_expected(
            result: Result<RefinedSearchConfig, SearchApiError>,
            expected_engine_orders: Vec<String>,
            message: &str,
        ) {
            assert!(
                result.is_ok(),
                "Should have filtered the configuration without error. {:?}",
                result
            );

            let refined_config = result.unwrap();
            let actual_engine_orders: Vec<String> = refined_config
                .engines
                .into_iter()
                .map(|e| e.identifier)
                .collect();

            assert_eq!(actual_engine_orders, expected_engine_orders, "{}", message);
        }

        assert_actual_engines_equals_expected(
            Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
                locale: "en-CA".into(),
                region: "CA".into(),
                ..Default::default()
            }),
            vec![
                "default-engine".to_string(),
                "default-private-engine".to_string(),
                "after-defaults".to_string(),
                "b-engine".to_string(),
                "a-engine".to_string(),
            ],
            "Should order the default engine first, default private engine second, and the rest of the engines based on order hint then alphabetically by name."
        );

        let starts_with_wiki_config = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::minimal("wiki-ca", "wiki-ca")
                .override_variants(
                    Variant::new()
                      .locales(&["en-CA"])
                      .regions(&["CA"])
                )
                .build(),
                EngineRecord::minimal("wiki-uk", "wiki-uk")
                .override_variants(
                    Variant::new()
                      .locales(&["en-GB"])
                      .regions(&["GB"])
                )
                .build(),
                EngineRecord::minimal("engine-1", "engine-1").build(),
                EngineRecord::minimal("engine-2", "engine-2").build(),
                {
                  "recordType": "engineOrders",
                  "orders": [
                    {
                      "environment": {
                        "locales": ["en-CA"],
                        "regions": ["CA"],
                      },
                      "order": ["wiki*", "engine-1", "engine-2"],
                    },
                    {
                      "environment": {
                        "locales": ["en-GB"],
                        "regions": ["GB"],
                      },
                      "order": ["wiki*", "engine-1", "engine-2"],
                    },
                  ],
                },
                {
                  "recordType": "availableLocales",
                  "locales": ["en-CA", "en-GB", "fr"]
                }

              ]
            })
            .to_string(),
        );
        starts_with_wiki_config.expect("Should have set the configuration successfully");

        assert_actual_engines_equals_expected(
            Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
                locale: "en-CA".into(),
                region: "CA".into(),
                ..Default::default()
            }),
            vec![
                "wiki-ca".to_string(),
                "engine-1".to_string(),
                "engine-2".to_string(),
            ],
            "Should list the wiki-ca engine and other engines in correct orders with the en-CA and CA locale region environment."
        );

        assert_actual_engines_equals_expected(
            Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
                locale: "en-GB".into(),
                region: "GB".into(),
                ..Default::default()
            }),
            vec![
                "wiki-uk".to_string(),
                "engine-1".to_string(),
                "engine-2".to_string(),
            ],
            "Should list the wiki-uk engine and other engines in correct orders with the en-GB and GB locale region environment."
        );
    }

    const APPLY_OVERRIDES: bool = true;
    const DO_NOT_APPLY_OVERRIDES: bool = false;
    const RECORDS_MISSING: bool = false;
    const RECORDS_PRESENT: bool = true;

    fn setup_remote_settings_test(
        should_apply_overrides: bool,
        expect_sync_successful: bool,
    ) -> Arc<SearchEngineSelector> {
        error_support::init_for_tests();
        viaduct_dev::init_backend_dev();

        let config = RemoteSettingsConfig2 {
            server: Some(RemoteSettingsServer::Custom {
                url: mockito::server_url(),
            }),
            bucket_name: Some(String::from("main")),
            app_context: Some(RemoteSettingsContext::default()),
        };
        let service = Arc::new(RemoteSettingsService::new(String::from(":memory:"), config));

        let selector = Arc::new(SearchEngineSelector::new());

        Arc::clone(&selector).use_remote_settings_server(&service, should_apply_overrides);
        let sync_result = Arc::clone(&service).sync();
        assert!(
            if expect_sync_successful {
                sync_result.is_ok()
            } else {
                sync_result.is_err()
            },
            "Should have completed the sync successfully. {:?}",
            sync_result
        );

        selector
    }

    fn mock_changes_endpoint() -> mockito::Mock {
        mock(
            "GET",
            "/v1/buckets/monitor/collections/changes/changeset?_expected=0",
        )
        .with_body(response_body_changes())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create()
    }

    fn response_body() -> String {
        json!({
          "metadata": {
            "id": "search-config-v2",
            "last_modified": 1000,
            "bucket": "main",
            "signature": {
              "x5u": "fake",
              "signature": "fake",
            },
          },
          "timestamp": 1000,
          "changes": [
            EngineRecord::minimal("test", "Test")
              .id("c5dcd1da-7126-4abb-846b-ec85b0d4d0d7")
              .schema(1001)
              .last_modified(1000)
              .build(),
            EngineRecord::minimal("distro-default", "Distribution Default")
              .id("c5dcd1da-7126-4abb-846b-ec85b0d4d0d8")
              .schema(1002)
              .last_modified(1000)
              .build(),
            EngineRecord::minimal("private-default-FR", "Private default FR")
              .id("c5dcd1da-7126-4abb-846b-ec85b0d4d0d9")
              .schema(1003)
              .last_modified(1000)
              .build(),
            {
              "recordType": "defaultEngines",
              "globalDefault": "test",
              "specificDefaults": [{
                "environment": {
                  "distributions": ["test-distro"],
                },
                "default": "distro-default"
              }, {
                "environment": {
                  "regions": ["fr"]
                },
                "defaultPrivate": "private-default-FR"
              }],
              "id": "c5dcd1da-7126-4abb-846b-ec85b0d4d0e0",
              "schema": 1004,
              "last_modified": 1000,
            }
          ]
        })
        .to_string()
    }

    fn response_body_changes() -> String {
        json!({
          "timestamp": 1000,
          "changes": [
            {
              "collection": "search-config-v2",
              "bucket": "main",
              "last_modified": 1000,
            }
        ],
        })
        .to_string()
    }

    fn response_body_locales() -> String {
        json!({
          "metadata": {
            "id": "search-config-v2",
            "last_modified": 1000,
            "bucket": "main",
            "signature": {
              "x5u": "fake",
              "signature": "fake",
            },
          },
          "timestamp": 1000,
          "changes": [
            EngineRecord::minimal("engine-de", "German Engine")
              .override_variants(
                Variant::new()
                  .locales(&["de"])
              )
              .id("c5dcd1da-7126-4abb-846b-ec85b0d4d0d7")
              .schema(1001)
              .last_modified(1000)
              .build(),
            EngineRecord::minimal("engine-en-us", "English US Engine")
              .override_variants(
                Variant::new()
                  .locales(&["en-US"])
              )
              .id("c5dcd1da-7126-4abb-846b-ec85b0d4d0d8")
              .schema(1002)
              .last_modified(1000)
              .build(),
            {
              "recordType": "availableLocales",
              "locales": ["de", "en-US"],
              "id": "c5dcd1da-7126-4abb-846b-ec85b0d4d0e0",
              "schema": 1004,
              "last_modified": 1000,
            }
          ]
        })
        .to_string()
    }

    fn response_body_overrides() -> String {
        let mut engine = test_helpers::overrides_engine();
        engine["identifier"] = json!("test");
        engine["id"] = json!("c5dcd1da-7126-4abb-846b-ec85b0d4d0d7");
        engine["schema"] = json!(1001);
        engine["last_modified"] = json!(1000);

        json!({
          "metadata": {
            "id": "search-config-overrides-v2",
            "last_modified": 1000,
            "bucket": "main",
            "signature": {
              "x5u": "fake",
              "signature": "fake",
            },
          },
          "timestamp": 1000,
          "changes": [ engine ]
        })
        .to_string()
    }

    #[test]
    fn test_remote_settings_empty_search_config_records_throws_error() {
        let changes_mock = mock_changes_endpoint();
        let m = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-v2/changeset?_expected=0",
        )
        .with_body(
            json!({
              "metadata": {
                "id": "search-config-v2",
                "last_modified": 1000,
                "bucket": "main",
                "signature": {
                  "x5u": "fake",
                  "signature": "fake",
                },
              },
              "timestamp": 1000,
              "changes": [
            ]})
            .to_string(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let selector = setup_remote_settings_test(DO_NOT_APPLY_OVERRIDES, RECORDS_PRESENT);

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: "test-distro".to_string(),
            ..Default::default()
        });
        assert!(
            result.is_err(),
            "Should throw an error when a configuration has not been specified before filtering"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No search config v2 records received from remote settings"));
        changes_mock.expect(1).assert();
        m.expect(1).assert();
    }

    #[test]
    fn test_remote_settings_search_config_records_is_none_throws_error() {
        let changes_mock = mock_changes_endpoint();
        let m1 = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-v2/changeset?_expected=0",
        )
        .with_body(response_body())
        .with_status(501)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let selector = setup_remote_settings_test(DO_NOT_APPLY_OVERRIDES, RECORDS_MISSING);

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: "test-distro".to_string(),
            ..Default::default()
        });
        assert!(
            result.is_err(),
            "Should throw an error when a configuration has not been specified before filtering"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No search config v2 records received from remote settings"));
        changes_mock.expect(1).assert();
        m1.expect(1).assert();
    }

    #[test]
    fn test_remote_settings_empty_search_config_overrides_filtered_without_error() {
        let changes_mock = mock_changes_endpoint();
        let m1 = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-v2/changeset?_expected=0",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let m2 = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-overrides-v2/changeset?_expected=0",
        )
        .with_body(
            json!({
               "metadata": {
                 "id": "search-config-overrides-v2",
                 "last_modified": 1000,
                 "bucket": "main",
                 "signature": {
                   "x5u": "fake",
                   "signature": "fake",
                 },
               },
               "timestamp": 1000,
               "changes": [
            ]})
            .to_string(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let selector = setup_remote_settings_test(APPLY_OVERRIDES, RECORDS_PRESENT);

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: "test-distro".to_string(),
            ..Default::default()
        });
        assert!(
            result.is_ok(),
            "Should have filtered the configuration using an empty search config overrides without causing an error. {:?}",
            result
        );
        changes_mock.expect(1).assert();
        m1.expect(1).assert();
        m2.expect(1).assert();
    }

    #[test]
    fn test_remote_settings_search_config_overrides_records_is_none_throws_error() {
        let changes_mock = mock_changes_endpoint();
        let m1 = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-v2/changeset?_expected=0",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let m2 = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-overrides-v2/changeset?_expected=0",
        )
        .with_body(response_body_overrides())
        .with_status(501)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let selector = setup_remote_settings_test(APPLY_OVERRIDES, RECORDS_MISSING);

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: "test-distro".to_string(),
            ..Default::default()
        });
        assert!(
            result.is_err(),
            "Should throw an error when a configuration overrides has not been specified before filtering"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No search config overrides v2 records received from remote settings"));
        changes_mock.expect(1).assert();
        m1.expect(1).assert();
        m2.expect(1).assert();
    }

    #[test]
    fn test_filter_with_remote_settings_overrides() {
        let changes_mock = mock_changes_endpoint();
        let m1 = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-v2/changeset?_expected=0",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let m2 = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-overrides-v2/changeset?_expected=0",
        )
        .with_body(response_body_overrides())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let selector = setup_remote_settings_test(APPLY_OVERRIDES, RECORDS_PRESENT);

        let override_test_engine = ExpectedEngine::minimal("test", "Test")
            .partner_code("overrides-partner-code")
            .click_url("https://example.com/click-url")
            .telemetry_suffix("overrides-telemetry-suffix")
            .search_base("https://example.com/search-overrides")
            .search_term_param_name("search")
            .search_params(vec![SearchUrlParam {
                name: "overrides-name".to_string(),
                value: Some("overrides-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }])
            .build();

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            ..Default::default()
        });

        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );
        assert_eq!(
            result.unwrap().engines[0],
            override_test_engine.clone(),
            "Should have applied the overrides to the matching engine"
        );
        changes_mock.expect(1).assert();
        m1.expect(1).assert();
        m2.expect(1).assert();
    }

    #[test]
    fn test_filter_with_remote_settings() {
        let changes_mock = mock_changes_endpoint();

        let m = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-v2/changeset?_expected=0",
        )
        .with_body(response_body())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let selector = setup_remote_settings_test(DO_NOT_APPLY_OVERRIDES, RECORDS_PRESENT);

        let test_engine = ExpectedEngine::minimal("test", "Test").build();
        let private_default_fr_engine =
            ExpectedEngine::minimal("private-default-FR", "Private default FR").build();
        let distro_default_engine =
            ExpectedEngine::minimal("distro-default", "Distribution Default").build();

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            distribution_id: "test-distro".to_string(),
            ..Default::default()
        });
        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );
        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec![
                    distro_default_engine.clone(),
                    private_default_fr_engine.clone(),
                    test_engine.clone(),
                ],
                app_default_engine_id: Some("distro-default".to_string()),
                app_private_default_engine_id: None
            },
            "Should have selected the default engine for the matching specific default"
        );

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            region: "fr".into(),
            distribution_id: String::new(),
            ..Default::default()
        });
        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );
        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec![
                    test_engine.clone(),
                    private_default_fr_engine.clone(),
                    distro_default_engine.clone(),
                ],
                app_default_engine_id: Some("test".to_string()),
                app_private_default_engine_id: Some("private-default-FR".to_string())
            },
            "Should have selected the private default engine for the matching specific default"
        );
        changes_mock.expect(1).assert();
        m.expect(1).assert();
    }

    #[test]
    fn test_filter_with_remote_settings_negotiate_locales() {
        let changes_mock = mock_changes_endpoint();
        let m = mock(
            "GET",
            "/v1/buckets/main/collections/search-config-v2/changeset?_expected=0",
        )
        .with_body(response_body_locales())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("etag", "\"1000\"")
        .create();

        let selector = setup_remote_settings_test(DO_NOT_APPLY_OVERRIDES, RECORDS_PRESENT);

        let result_de = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            locale: "de-AT".into(),
            ..Default::default()
        });
        assert!(
            result_de.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result_de
        );

        assert_eq!(
            result_de.unwrap(),
            RefinedSearchConfig {
                engines: vec![ExpectedEngine::minimal("engine-de", "German Engine").build()],
                app_default_engine_id: None,
                app_private_default_engine_id: None,
            },
            "Should have selected the de engine when given de-AT which is not an available locale"
        );

        let result_en = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            locale: "en-AU".to_string(),
            ..Default::default()
        });
        assert_eq!(
            result_en.unwrap(),
            RefinedSearchConfig {
                engines: vec![ExpectedEngine::minimal("engine-en-us", "English US Engine").build(),],
                app_default_engine_id: None,
                app_private_default_engine_id: None,
            },
            "Should have selected the en-us engine when given another english locale we don't support"
        );
        changes_mock.expect(1).assert();
        m.expect(1).assert();
    }

    #[test]
    fn test_configuration_overrides_applied() {
        let selector = Arc::new(SearchEngineSelector::new());

        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({
              "data": [
                test_helpers::overrides_engine(),
                { // Test partial override with some missing fields
                  "identifier": "distro-default",
                  "partnerCode": "distro-overrides-partner-code",
                  "clickUrl": "https://example.com/click-url-distro",
                  "urls": {
                    "search": {
                      "base": "https://example.com/search-distro",
                    },
                  },
                }
              ]
            })
            .to_string(),
        );
        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                EngineRecord::minimal("overrides-engine", "Overrides Engine")
                    .build(),
                EngineRecord::minimal("distro-default", "Distribution Default")
                    .override_variants(Variant::new()
                    .all_regions_and_locales()
                    .telemetry_suffix("distro-telemetry-suffix"))
                    .build(),
              ]
            })
            .to_string(),
        );
        config_result.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        let override_test_engine = ExpectedEngine::minimal("overrides-engine", "Overrides Engine")
            .partner_code("overrides-partner-code")
            .click_url("https://example.com/click-url")
            .telemetry_suffix("overrides-telemetry-suffix")
            .search_base("https://example.com/search-overrides")
            .search_params(vec![SearchUrlParam {
                name: "overrides-name".to_string(),
                value: Some("overrides-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }])
            .build();

        let override_distro_default_engine =
            ExpectedEngine::minimal("distro-default", "Distribution Default")
                .partner_code("distro-overrides-partner-code")
                .click_url("https://example.com/click-url-distro")
                .search_base("https://example.com/search-distro")
                .telemetry_suffix("distro-telemetry-suffix")
                .build();

        let result = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            ..Default::default()
        });
        assert!(
            result.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result
        );

        assert_eq!(
            result.unwrap(),
            RefinedSearchConfig {
                engines: vec![
                    override_distro_default_engine.clone(),
                    override_test_engine.clone(),
                ],
                app_default_engine_id: None,
                app_private_default_engine_id: None
            },
            "Should have applied the overrides to the matching engine."
        );
    }

    #[test]
    fn test_filter_engine_configuration_negotiate_locales() {
        let selector = Arc::new(SearchEngineSelector::new());
        let config_overrides_result = Arc::clone(&selector).set_config_overrides(
            json!({ "data": [test_helpers::overrides_engine()] }).to_string(),
        );

        let config_result = Arc::clone(&selector).set_search_config(
            json!({
              "data": [
                {
                    "recordType": "availableLocales",
                    "locales": ["de", "en-US"]
                },
                EngineRecord::minimal("engine-de", "German Engine")
                    .override_variants(Variant::new()
                    .locales(&["de"]))
                    .build(),
                EngineRecord::minimal("engine-en-us", "English US Engine")
                .override_variants(Variant::new()
                    .locales(&["en-US"]))
                    .build(),
              ]
            })
            .to_string(),
        );
        config_result.expect("Should have set the configuration successfully");
        config_overrides_result.expect("Should have set the configuration overrides successfully");

        let result_de = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            locale: "de-AT".into(),
            ..Default::default()
        });
        assert!(
            result_de.is_ok(),
            "Should have filtered the configuration without error. {:?}",
            result_de
        );

        assert_eq!(
            result_de.unwrap(),
            RefinedSearchConfig {
                engines: vec![ExpectedEngine::minimal("engine-de", "German Engine").build(),],
                app_default_engine_id: None,
                app_private_default_engine_id: None,
            },
            "Should have selected the de engine when given de-AT which is not an available locale"
        );

        let result_en = Arc::clone(&selector).filter_engine_configuration(SearchUserEnvironment {
            locale: "en-AU".to_string(),
            ..Default::default()
        });
        assert_eq!(
            result_en.unwrap(),
            RefinedSearchConfig {
                engines: vec![ExpectedEngine::minimal("engine-en-us", "English US Engine").build(),],
                app_default_engine_id: None,
                app_private_default_engine_id: None,
            },
            "Should have selected the en-us engine when given another english locale we don't support"
        );
    }
}
