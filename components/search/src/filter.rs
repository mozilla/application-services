/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines the functions for managing the filtering of the configuration.

use crate::configuration_overrides_types::JSONOverridesRecord;
use crate::environment_matching::matches_user_environment;
use crate::{
    error::Error, JSONDefaultEnginesRecord, JSONEngineBase, JSONEngineMethod, JSONEngineRecord,
    JSONEngineUrl, JSONEngineUrls, JSONEngineVariant, JSONSearchConfigurationRecords,
    RefinedSearchConfig, SearchEngineDefinition, SearchEngineUrl, SearchEngineUrls,
    SearchUserEnvironment,
};
use crate::{sort_helpers, JSONAvailableLocalesRecord, JSONEngineOrdersRecord};
use remote_settings::RemoteSettingsRecord;
use std::collections::HashSet;

impl Default for SearchEngineUrl {
    fn default() -> Self {
        Self {
            base: Default::default(),
            method: JSONEngineMethod::default().as_str().to_string(),
            params: Default::default(),
            search_term_param_name: Default::default(),
            display_name: Default::default(),
            is_new_until: Default::default(),
            exclude_partner_code_from_telemetry: Default::default(),
            accepted_content_types: Default::default(),
        }
    }
}

impl SearchEngineUrl {
    fn merge(&mut self, user_environment: &SearchUserEnvironment, preferred: &JSONEngineUrl) {
        if let Some(base) = &preferred.base {
            self.base = base.clone();
        }
        if let Some(method) = &preferred.method {
            self.method = method.as_str().to_string();
        }
        if let Some(params) = &preferred.params {
            self.params = params.clone();
        }
        if let Some(search_term_param_name) = &preferred.search_term_param_name {
            self.search_term_param_name = Some(search_term_param_name.clone());
        }
        if let Some(display_name_map) = &preferred.display_name_map {
            self.display_name = display_name_map
                .get(&user_environment.locale)
                .or_else(|| display_name_map.get("default"))
                .cloned();
        }
        if let Some(is_new_until) = &preferred.is_new_until {
            self.is_new_until = Some(is_new_until.clone());
        }
        if let Some(accepted_content_types) = &preferred.accepted_content_types {
            self.accepted_content_types = Some(accepted_content_types.clone());
        }
        self.exclude_partner_code_from_telemetry = preferred.exclude_partner_code_from_telemetry;
    }
}

impl SearchEngineUrls {
    fn merge(&mut self, user_environment: &SearchUserEnvironment, preferred: &JSONEngineUrls) {
        if let Some(search_url) = &preferred.search {
            self.search.merge(user_environment, search_url);
        }
        if let Some(suggestions) = &preferred.suggestions {
            self.suggestions
                .get_or_insert_with(Default::default)
                .merge(user_environment, suggestions);
        }
        if let Some(trending) = &preferred.trending {
            self.trending
                .get_or_insert_with(Default::default)
                .merge(user_environment, trending);
        }
        if let Some(search_form) = &preferred.search_form {
            self.search_form
                .get_or_insert_with(Default::default)
                .merge(user_environment, search_form);
        }
        if let Some(visual_search) = &preferred.visual_search {
            self.visual_search
                .get_or_insert_with(Default::default)
                .merge(user_environment, visual_search);
        }
    }
}

impl SearchEngineDefinition {
    fn merge_variant(
        &mut self,
        user_environment: &SearchUserEnvironment,
        variant: &JSONEngineVariant,
    ) {
        if !self.optional {
            self.optional = variant.optional;
        }
        if let Some(partner_code) = &variant.partner_code {
            self.partner_code = partner_code.clone();
        }
        if let Some(telemetry_suffix) = &variant.telemetry_suffix {
            self.telemetry_suffix = telemetry_suffix.clone();
        }
        if let Some(urls) = &variant.urls {
            self.urls.merge(user_environment, urls);
        }
        if let Some(is_new_until) = &variant.is_new_until {
            self.is_new_until = Some(is_new_until.clone());
        }
    }

    fn merge_override(
        &mut self,
        user_environment: &SearchUserEnvironment,
        override_record: &JSONOverridesRecord,
    ) {
        self.partner_code = override_record.partner_code.clone();
        self.urls.merge(user_environment, &override_record.urls);
        self.click_url = Some(override_record.click_url.clone());

        if let Some(telemetry_suffix) = &override_record.telemetry_suffix {
            self.telemetry_suffix = telemetry_suffix.clone();
        }
    }

    pub(crate) fn from_configuration_details(
        user_environment: &SearchUserEnvironment,
        identifier: &str,
        base: JSONEngineBase,
        variant: &JSONEngineVariant,
        sub_variant: &Option<JSONEngineVariant>,
    ) -> SearchEngineDefinition {
        let mut engine_definition = SearchEngineDefinition {
            aliases: base.aliases.unwrap_or_default(),
            charset: base.charset.unwrap_or_else(|| "UTF-8".to_string()),
            classification: base.classification,
            identifier: identifier.to_string(),
            name: base.name,
            optional: variant.optional,
            order_hint: None,
            partner_code: base.partner_code.unwrap_or_default(),
            telemetry_suffix: String::new(),
            urls: SearchEngineUrls::default(),
            click_url: None,
            is_new_until: None,
        };

        engine_definition.urls.merge(user_environment, &base.urls);
        engine_definition.merge_variant(user_environment, variant);
        if let Some(sub_variant) = sub_variant {
            engine_definition.merge_variant(user_environment, sub_variant);
        }

        engine_definition
    }
}

pub(crate) struct FilterRecordsResult {
    engines: Vec<SearchEngineDefinition>,
    default_engines_record: Option<JSONDefaultEnginesRecord>,
    engine_orders_record: Option<JSONEngineOrdersRecord>,
}

pub(crate) trait Filter {
    fn filter_records(
        &self,
        user_environment: &mut SearchUserEnvironment,
        overrides: Option<Vec<JSONOverridesRecord>>,
    ) -> Result<FilterRecordsResult, Error>;
}

fn apply_overrides(
    user_environment: &SearchUserEnvironment,
    engines: &mut [SearchEngineDefinition],
    overrides: &[JSONOverridesRecord],
) {
    for override_record in overrides {
        for engine in engines.iter_mut() {
            if engine.identifier == override_record.identifier {
                engine.merge_override(user_environment, override_record);
            }
        }
    }
}

fn negotiate_languages(user_environment: &mut SearchUserEnvironment, available_locales: &[String]) {
    let user_locale = user_environment.locale.to_lowercase();

    let available_locales_set: HashSet<String> = available_locales
        .iter()
        .map(|locale| locale.to_lowercase())
        .collect();

    if available_locales_set.contains(&user_locale) {
        return;
    }
    if user_locale.starts_with("en-") {
        user_environment.locale = "en-us".to_string();
        return;
    }
    if let Some(index) = user_locale.find('-') {
        let base_locale = &user_locale[..index];
        if available_locales_set.contains(base_locale) {
            user_environment.locale = base_locale.to_string();
        }
    }
}

impl Filter for Vec<RemoteSettingsRecord> {
    fn filter_records(
        &self,
        user_environment: &mut SearchUserEnvironment,
        overrides: Option<Vec<JSONOverridesRecord>>,
    ) -> Result<FilterRecordsResult, Error> {
        let mut available_locales = Vec::new();
        for record in self {
            if let Some(val) = record.fields.get("recordType") {
                if *val == "availableLocales" {
                    let stringified = serde_json::to_string(&record.fields)?;
                    let locales_record: Option<JSONAvailableLocalesRecord> =
                        serde_json::from_str(&stringified)?;
                    available_locales = locales_record.unwrap().locales;
                }
            }
        }
        negotiate_languages(user_environment, &available_locales);

        let mut engines = Vec::new();
        let mut default_engines_record = None;
        let mut engine_orders_record = None;

        for record in self {
            // TODO: Bug 1947241 - Find a way to avoid having to serialise the records
            // back to strings and then deserialise them into the records that we want.
            let stringified = serde_json::to_string(&record.fields)?;
            match record.fields.get("recordType") {
                Some(val) if *val == "engine" => {
                    let engine_config: Option<JSONEngineRecord> =
                        serde_json::from_str(&stringified)?;
                    if let Some(engine_config) = engine_config {
                        let result =
                            maybe_extract_engine_config(user_environment, Box::new(engine_config));
                        engines.extend(result);
                    }
                }
                Some(val) if *val == "defaultEngines" => {
                    default_engines_record = serde_json::from_str(&stringified)?;
                }
                Some(val) if *val == "engineOrders" => {
                    engine_orders_record = serde_json::from_str(&stringified)?;
                }
                Some(val) if *val == "availableLocales" => {
                    // Handled above
                }
                // These cases are acceptable - we expect the potential for new
                // record types/options so that we can be flexible.
                Some(_val) => {}
                None => {}
            }
        }

        if let Some(overrides_data) = &overrides {
            apply_overrides(user_environment, &mut engines, overrides_data);
        }

        Ok(FilterRecordsResult {
            engines,
            default_engines_record,
            engine_orders_record,
        })
    }
}

impl Filter for Vec<JSONSearchConfigurationRecords> {
    fn filter_records(
        &self,
        user_environment: &mut SearchUserEnvironment,
        overrides: Option<Vec<JSONOverridesRecord>>,
    ) -> Result<FilterRecordsResult, Error> {
        let mut available_locales = Vec::new();
        for record in self {
            if let JSONSearchConfigurationRecords::AvailableLocales(locales_record) = record {
                available_locales = locales_record.locales.clone();
            }
        }
        negotiate_languages(user_environment, &available_locales);

        let mut engines = Vec::new();
        let mut default_engines_record = None;
        let mut engine_orders_record = None;

        for record in self {
            match record {
                JSONSearchConfigurationRecords::Engine(engine) => {
                    let result = maybe_extract_engine_config(user_environment, engine.clone());
                    engines.extend(result);
                }
                JSONSearchConfigurationRecords::DefaultEngines(default_engines) => {
                    default_engines_record = Some(default_engines);
                }
                JSONSearchConfigurationRecords::EngineOrders(engine_orders) => {
                    engine_orders_record = Some(engine_orders)
                }
                JSONSearchConfigurationRecords::AvailableLocales(_) => {
                    // Handled above
                }
                JSONSearchConfigurationRecords::Unknown => {
                    // Prevents panics if a new record type is added in future.
                }
            }
        }

        if let Some(overrides_data) = &overrides {
            apply_overrides(user_environment, &mut engines, overrides_data);
        }

        Ok(FilterRecordsResult {
            engines,
            default_engines_record: default_engines_record.cloned(),
            engine_orders_record: engine_orders_record.cloned(),
        })
    }
}

pub(crate) fn filter_engine_configuration_impl(
    user_environment: SearchUserEnvironment,
    configuration: &impl Filter,
    overrides: Option<Vec<JSONOverridesRecord>>,
) -> Result<RefinedSearchConfig, Error> {
    let mut user_environment = user_environment.clone();
    user_environment.locale = user_environment.locale.to_lowercase();
    user_environment.region = user_environment.region.to_lowercase();
    user_environment.version = user_environment.version.to_lowercase();

    let filtered_result = configuration.filter_records(&mut user_environment, overrides);

    filtered_result.map(|result| {
        let (default_engine_id, default_private_engine_id) = determine_default_engines(
            &result.engines,
            result.default_engines_record,
            &user_environment,
        );

        let mut engines = result.engines.clone();

        if let Some(orders_record) = result.engine_orders_record {
            for order_data in &orders_record.orders {
                if matches_user_environment(&order_data.environment, &user_environment) {
                    sort_helpers::set_engine_order(&mut engines, &order_data.order);
                }
            }
        }

        engines.sort_by(|a, b| {
            sort_helpers::sort(
                default_engine_id.as_ref(),
                default_private_engine_id.as_ref(),
                a,
                b,
            )
        });

        RefinedSearchConfig {
            engines,
            app_default_engine_id: default_engine_id,
            app_private_default_engine_id: default_private_engine_id,
        }
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

    let mut matching_sub_variant = None;
    if let Some(variant) = &matching_variant {
        matching_sub_variant = variant
            .sub_variants
            .iter()
            .rev()
            .find(|r| matches_user_environment(&r.environment, user_environment))
            .cloned();
    }

    matching_variant.map(|variant| {
        SearchEngineDefinition::from_configuration_details(
            user_environment,
            &identifier,
            base,
            &variant,
            &matching_sub_variant,
        )
    })
}

fn determine_default_engines(
    engines: &[SearchEngineDefinition],
    default_engines_record: Option<JSONDefaultEnginesRecord>,
    user_environment: &SearchUserEnvironment,
) -> (Option<String>, Option<String>) {
    match default_engines_record {
        None => (None, None),
        Some(record) => {
            let mut default_engine_id = None;
            let mut default_engine_private_id = None;

            let specific_default = record
                .specific_defaults
                .into_iter()
                .rev()
                .find(|r| matches_user_environment(&r.environment, user_environment));

            if let Some(specific_default) = specific_default {
                // Check the engine is present in the list of engines before
                // we return it as default.
                if let Some(engine_id) =
                    find_engine_id_with_match(engines, specific_default.default)
                {
                    default_engine_id.replace(engine_id);
                }
                if let Some(private_engine_id) =
                    find_engine_id_with_match(engines, specific_default.default_private)
                {
                    default_engine_private_id.replace(private_engine_id);
                }
            }

            (
                // If we haven't found a default engine in a specific default,
                // then fall back to the global default engine - but only if that
                // exists in the engine list.
                //
                // For the normal mode engine (`default_engine_id`), this would
                // effectively be considered an error. However, we can't do anything
                // sensible here, so we will return `None` to the application, and
                // that can handle it.
                default_engine_id.or_else(|| find_engine_id(engines, record.global_default)),
                default_engine_private_id
                    .or_else(|| find_engine_id(engines, record.global_default_private)),
            )
        }
    }
}

fn find_engine_id(engines: &[SearchEngineDefinition], engine_id: String) -> Option<String> {
    if engine_id.is_empty() {
        return None;
    }
    match engines.iter().any(|e| e.identifier == engine_id) {
        true => Some(engine_id.clone()),
        false => None,
    }
}

fn find_engine_id_with_match(
    engines: &[SearchEngineDefinition],
    engine_id_match: String,
) -> Option<String> {
    if engine_id_match.is_empty() {
        return None;
    }
    if let Some(match_no_star) = engine_id_match.strip_suffix('*') {
        return engines
            .iter()
            .find(|e| e.identifier.starts_with(match_no_star))
            .map(|e| e.identifier.clone());
    }

    engines
        .iter()
        .find(|e| e.identifier == engine_id_match)
        .map(|e| e.identifier.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use once_cell::sync::Lazy;
    use std::{collections::HashMap, vec};

    #[test]
    fn test_default_search_engine_url() {
        assert_eq!(
            SearchEngineUrl::default(),
            SearchEngineUrl {
                base: "".to_string(),
                method: "GET".to_string(),
                params: Vec::new(),
                search_term_param_name: None,
                display_name: None,
                is_new_until: None,
                exclude_partner_code_from_telemetry: false,
                accepted_content_types: None,
            },
        );
    }

    #[test]
    fn test_default_search_engine_urls() {
        assert_eq!(
            SearchEngineUrls::default(),
            SearchEngineUrls {
                search: SearchEngineUrl::default(),
                suggestions: None,
                trending: None,
                search_form: None,
                visual_search: None,
            },
        );
    }

    #[test]
    fn test_merge_override() {
        let mut test_engine = SearchEngineDefinition {
            identifier: "test".to_string(),
            partner_code: "partner-code".to_string(),
            telemetry_suffix: "original-telemetry-suffix".to_string(),
            ..Default::default()
        };

        let override_record = JSONOverridesRecord {
            identifier: "test".to_string(),
            partner_code: "override-partner-code".to_string(),
            click_url: "https://example.com/click-url".to_string(),
            telemetry_suffix: None,
            urls: JSONEngineUrls {
                search: Some(JSONEngineUrl {
                    base: Some("https://example.com/override-search".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        test_engine.merge_override(
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
            &override_record,
        );

        assert_eq!(
            test_engine.partner_code, "override-partner-code",
            "Should override the partner code"
        );
        assert_eq!(
            test_engine.click_url,
            Some("https://example.com/click-url".to_string()),
            "Should override the click url"
        );
        assert_eq!(
            test_engine.urls.search.base, "https://example.com/override-search",
            "Should override search url"
        );
        assert_eq!(
            test_engine.telemetry_suffix, "original-telemetry-suffix",
            "Should not override telemetry suffix when telemetry suffix is supplied as None"
        );
    }

    #[test]
    fn test_merge_override_locale_match() {
        let mut test_engine = SearchEngineDefinition {
            identifier: "test".to_string(),
            partner_code: "partner-code".to_string(),
            telemetry_suffix: "original-telemetry-suffix".to_string(),
            ..Default::default()
        };

        let override_record = JSONOverridesRecord {
            identifier: "test".to_string(),
            partner_code: "override-partner-code".to_string(),
            click_url: "https://example.com/click-url".to_string(),
            telemetry_suffix: None,
            urls: JSONEngineUrls {
                search: Some(JSONEngineUrl {
                    base: Some("https://example.com/override-search".to_string()),
                    display_name_map: Some(HashMap::from([
                        // Default display name
                        ("default".to_string(), "My Display Name".to_string()),
                        // en-GB locale with unique display name
                        ("en-GB".to_string(), "en-GB Display Name".to_string()),
                    ])),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        test_engine.merge_override(
            &SearchUserEnvironment {
                // en-GB locale
                locale: "en-GB".into(),
                ..Default::default()
            },
            &override_record,
        );

        assert_eq!(
            test_engine.urls.search.display_name,
            Some("en-GB Display Name".to_string()),
            "Should override display name with en-GB version"
        );
    }

    static ENGINES_LIST: Lazy<Vec<SearchEngineDefinition>> = Lazy::new(|| {
        vec![
            SearchEngineDefinition {
                identifier: "engine1".to_string(),
                name: "Test".to_string(),
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            SearchEngineDefinition {
                identifier: "engine2".to_string(),
                name: "Test 2".to_string(),
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com/2".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            SearchEngineDefinition {
                identifier: "engine3".to_string(),
                name: "Test 3".to_string(),
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com/3".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            SearchEngineDefinition {
                identifier: "engine4wildcardmatch".to_string(),
                name: "Test 4".to_string(),
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com/4".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        ]
    });

    #[test]
    fn test_determine_default_engines_returns_global_default() {
        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: String::new(),
                specific_defaults: Vec::new(),
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine2",
            "Should have returned the global default engine"
        );
        assert!(
            default_engine_private_id.is_none(),
            "Should not have returned an id for the private engine"
        );

        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: String::new(),
                specific_defaults: vec![JSONSpecificDefaultRecord {
                    default: "engine1".to_string(),
                    default_private: String::new(),
                    environment: JSONVariantEnvironment {
                        locales: vec!["en-GB".to_string()],
                        ..Default::default()
                    },
                }],
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine2",
            "Should have returned the global default engine when no specific defaults environments match"
        );
        assert!(
            default_engine_private_id.is_none(),
            "Should not have returned an id for the private engine"
        );

        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: String::new(),
                specific_defaults: vec![JSONSpecificDefaultRecord {
                    default: "engine1".to_string(),
                    default_private: String::new(),
                    environment: JSONVariantEnvironment {
                        locales: vec!["fi".to_string()],
                        ..Default::default()
                    },
                }],
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine1",
            "Should have returned the specific default when environments match"
        );
        assert!(
            default_engine_private_id.is_none(),
            "Should not have returned an id for the private engine"
        );

        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: String::new(),
                specific_defaults: vec![JSONSpecificDefaultRecord {
                    default: "engine4*".to_string(),
                    default_private: String::new(),
                    environment: JSONVariantEnvironment {
                        locales: vec!["fi".to_string()],
                        ..Default::default()
                    },
                }],
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine4wildcardmatch",
            "Should have returned the specific default when using a wildcard match"
        );
        assert!(
            default_engine_private_id.is_none(),
            "Should not have returned an id for the private engine"
        );

        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: String::new(),
                specific_defaults: vec![
                    JSONSpecificDefaultRecord {
                        default: "engine4*".to_string(),
                        default_private: String::new(),
                        environment: JSONVariantEnvironment {
                            locales: vec!["fi".to_string()],
                            ..Default::default()
                        },
                    },
                    JSONSpecificDefaultRecord {
                        default: "engine3".to_string(),
                        default_private: String::new(),
                        environment: JSONVariantEnvironment {
                            locales: vec!["fi".to_string()],
                            ..Default::default()
                        },
                    },
                ],
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine3",
            "Should have returned the last specific default when multiple environments match"
        );
        assert!(
            default_engine_private_id.is_none(),
            "Should not have returned an id for the private engine"
        );
    }

    #[test]
    fn test_determine_default_engines_returns_global_default_private() {
        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: "engine3".to_string(),
                specific_defaults: Vec::new(),
            }),
            &SearchUserEnvironment {
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine2",
            "Should have returned the global default engine"
        );
        assert_eq!(
            default_engine_private_id.unwrap(),
            "engine3",
            "Should have returned the global default engine for private mode"
        );

        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: "engine3".to_string(),
                specific_defaults: vec![JSONSpecificDefaultRecord {
                    default: String::new(),
                    default_private: "engine1".to_string(),
                    environment: JSONVariantEnvironment {
                        locales: vec!["en-GB".to_string()],
                        ..Default::default()
                    },
                }],
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine2",
            "Should have returned the global default engine when no specific defaults environments match"
        );
        assert_eq!(
            default_engine_private_id.unwrap(),
            "engine3",
            "Should have returned the global default engine for private mode when no specific defaults environments match"
        );

        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: "engine3".to_string(),
                specific_defaults: vec![JSONSpecificDefaultRecord {
                    default: String::new(),
                    default_private: "engine1".to_string(),
                    environment: JSONVariantEnvironment {
                        locales: vec!["fi".to_string()],
                        ..Default::default()
                    },
                }],
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine2",
            "Should have returned the global default engine when specific environments match which override the private global default (and not the global default)."
        );
        assert_eq!(
            default_engine_private_id.unwrap(),
            "engine1",
            "Should have returned the specific default engine for private mode when environments match"
        );

        let (default_engine_id, default_engine_private_id) = determine_default_engines(
            &ENGINES_LIST,
            Some(JSONDefaultEnginesRecord {
                global_default: "engine2".to_string(),
                global_default_private: String::new(),
                specific_defaults: vec![JSONSpecificDefaultRecord {
                    default: String::new(),
                    default_private: "engine4*".to_string(),
                    environment: JSONVariantEnvironment {
                        locales: vec!["fi".to_string()],
                        ..Default::default()
                    },
                }],
            }),
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
        );

        assert_eq!(
            default_engine_id.unwrap(),
            "engine2",
            "Should have returned the global default engine when specific environments match which override the private global default (and not the global default)"
        );
        assert_eq!(
            default_engine_private_id.unwrap(),
            "engine4wildcardmatch",
            "Should have returned the specific default for private mode when using a wildcard match"
        );
    }

    #[test]
    fn test_locale_matched_exactly() {
        let mut user_env = SearchUserEnvironment {
            locale: "en-CA".into(),
            ..Default::default()
        };
        negotiate_languages(&mut user_env, &["en-CA".to_string(), "fr".to_string()]);
        assert_eq!(
            user_env.locale, "en-CA",
            "Should return user locale unchanged if in available locales"
        );
    }

    #[test]
    fn test_locale_fallback_to_base_locale() {
        let mut user_env = SearchUserEnvironment {
            locale: "de-AT".into(),
            ..Default::default()
        };
        negotiate_languages(&mut user_env, &["de".to_string()]);
        assert_eq!(
            user_env.locale, "de",
            "Should fallback to base locale if base is in available locales"
        );
    }

    static ENGLISH_LOCALES: &[&str] = &["en-AU", "en-IE", "en-RU", "en-ZA"];

    #[test]
    fn test_english_locales_fallbacks_to_en_us() {
        for user_locale in ENGLISH_LOCALES {
            let mut user_env = SearchUserEnvironment {
                locale: user_locale.to_string(),
                ..Default::default()
            };
            negotiate_languages(&mut user_env, &["en-US".to_string()]);
            assert_eq!(
                user_env.locale, "en-us",
                "Should remap {} to en-us when en-us is available",
                user_locale
            );
        }
    }

    #[test]
    fn test_locale_unmatched() {
        let mut user_env = SearchUserEnvironment {
            locale: "fr-CA".into(),
            ..Default::default()
        };
        negotiate_languages(&mut user_env, &["de".to_string(), "en-US".to_string()]);
        assert_eq!(
            user_env.locale, "fr-CA",
            "Should leave locale unchanged if no match or english locale fallback is not found"
        );
    }
}

#[cfg(test)]
mod from_configuration_details_tests {
    use crate::test_helpers::{
        ExpectedEngineFromJSONBase, JSON_ENGINE_BASE, JSON_ENGINE_SUBVARIANT, JSON_ENGINE_VARIANT,
    };
    use crate::*;
    use once_cell::sync::Lazy;

    #[test]
    fn test_fallsback_to_defaults() {
        // This test doesn't use `..Default::default()` as we want to
        // be explicit about `JSONEngineBase` and handling `None`
        // options/default values.
        let result = SearchEngineDefinition::from_configuration_details(
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
            "test",
            JSONEngineBase {
                aliases: None,
                charset: None,
                classification: SearchEngineClassification::General,
                name: "Test".to_string(),
                partner_code: None,
                urls: JSONEngineUrls {
                    search: Some(JSONEngineUrl {
                        base: Some("https://example.com".to_string()),
                        ..Default::default()
                    }),
                    suggestions: None,
                    trending: None,
                    search_form: None,
                    visual_search: None,
                },
            },
            &JSONEngineVariant {
                environment: JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    ..Default::default()
                },
                is_new_until: None,
                optional: false,
                partner_code: None,
                telemetry_suffix: None,
                urls: None,
                sub_variants: vec![],
            },
            &None,
        );

        assert_eq!(
            result,
            SearchEngineDefinition {
                aliases: Vec::new(),
                charset: "UTF-8".to_string(),
                classification: SearchEngineClassification::General,
                identifier: "test".to_string(),
                is_new_until: None,
                partner_code: String::new(),
                name: "Test".to_string(),
                optional: false,
                order_hint: None,
                telemetry_suffix: String::new(),
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com".to_string(),
                        ..Default::default()
                    },
                    suggestions: None,
                    trending: None,
                    search_form: None,
                    visual_search: None,
                },
                click_url: None
            }
        )
    }

    #[test]
    fn test_uses_base_values_only() {
        let result = SearchEngineDefinition::from_configuration_details(
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
            "test",
            Lazy::force(&JSON_ENGINE_BASE).clone(),
            &JSONEngineVariant {
                environment: JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    ..Default::default()
                },
                is_new_until: None,
                optional: false,
                partner_code: None,
                telemetry_suffix: None,
                urls: None,
                sub_variants: vec![],
            },
            &None,
        );
        assert_eq!(
            result,
            ExpectedEngineFromJSONBase::new("test", "Test").build()
        );
    }

    #[test]
    fn test_uses_locale_specific_visual_display_name() {
        let result = SearchEngineDefinition::from_configuration_details(
            &SearchUserEnvironment {
                locale: "en-GB".into(),
                ..Default::default()
            },
            "test",
            Lazy::force(&JSON_ENGINE_BASE).clone(),
            &JSONEngineVariant {
                environment: JSONVariantEnvironment {
                    all_regions_and_locales: true,
                    ..Default::default()
                },
                is_new_until: None,
                optional: false,
                partner_code: None,
                telemetry_suffix: None,
                urls: None,
                sub_variants: vec![],
            },
            &None,
        );

        assert_eq!(
            result,
            ExpectedEngineFromJSONBase::new("test", "Test")
                .visual_search_display_name("Visual Search en-GB")
                .build()
        );
    }

    #[test]
    fn test_merges_variants() {
        let result = SearchEngineDefinition::from_configuration_details(
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
            "test",
            Lazy::force(&JSON_ENGINE_BASE).clone(),
            &JSON_ENGINE_VARIANT,
            &None,
        );

        assert_eq!(
            result,
            ExpectedEngineFromJSONBase::new("test", "Test")
                .variant_is_new_until("2063-04-05")
                .variant_optional(true)
                .variant_partner_code("trek")
                .variant_telemetry_suffix("star")
                .variant_search_url(
                    "https://example.com/variant",
                    "GET",
                    "variant",
                    "test variant",
                    "ship",
                )
                .variant_suggestions_url(
                    "https://example.com/suggestions-variant",
                    "GET",
                    "suggest-variant",
                    "sugg test variant",
                    "variant",
                )
                .variant_trending_url(
                    "https://example.com/trending-variant",
                    "GET",
                    "trend-variant",
                    "trend test variant",
                    "trend",
                    true,
                )
                .variant_search_form_url(
                    "https://example.com/search_form",
                    "GET",
                    "search-form-name",
                    "search-form-value",
                )
                .variant_visual_search_url(
                    "https://example.com/visual-search-variant",
                    "visual-search-variant-name",
                    "visual-search-variant-value",
                    "url_variant",
                    "Visual Search Variant",
                    "2096-02-02",
                )
                .build()
        );
    }

    #[test]
    fn test_merges_variant_and_uses_locale_specific_visual_search_display_name() {
        let result = SearchEngineDefinition::from_configuration_details(
            &SearchUserEnvironment {
                locale: "en-GB".into(),
                ..Default::default()
            },
            "test",
            Lazy::force(&JSON_ENGINE_BASE).clone(),
            &JSON_ENGINE_VARIANT,
            &None,
        );

        assert_eq!(
            result,
            ExpectedEngineFromJSONBase::new("test", "Test")
                .variant_is_new_until("2063-04-05")
                .variant_optional(true)
                .variant_partner_code("trek")
                .variant_telemetry_suffix("star")
                .variant_search_url(
                    "https://example.com/variant",
                    "GET",
                    "variant",
                    "test variant",
                    "ship",
                )
                .variant_suggestions_url(
                    "https://example.com/suggestions-variant",
                    "GET",
                    "suggest-variant",
                    "sugg test variant",
                    "variant",
                )
                .variant_trending_url(
                    "https://example.com/trending-variant",
                    "GET",
                    "trend-variant",
                    "trend test variant",
                    "trend",
                    true,
                )
                .variant_search_form_url(
                    "https://example.com/search_form",
                    "GET",
                    "search-form-name",
                    "search-form-value",
                )
                .variant_visual_search_url(
                    "https://example.com/visual-search-variant",
                    "visual-search-variant-name",
                    "visual-search-variant-value",
                    "url_variant",
                    // locale-specific display name is the key difference here
                    "Visual Search Variant en-GB",
                    "2096-02-02",
                )
                .build()
        );
    }

    #[test]
    fn test_merges_sub_variants() {
        let result = SearchEngineDefinition::from_configuration_details(
            &SearchUserEnvironment {
                locale: "fi".into(),
                ..Default::default()
            },
            "test",
            Lazy::force(&JSON_ENGINE_BASE).clone(),
            &JSON_ENGINE_VARIANT,
            &Some(JSON_ENGINE_SUBVARIANT.clone()),
        );

        assert_eq!(
            result,
            ExpectedEngineFromJSONBase::new("test", "Test")
                .variant_is_new_until("2063-04-05")
                .variant_optional(true)
                .subvariant_partner_code("trek2")
                .subvariant_telemetry_suffix("star2")
                .subvariant_search_url(
                    "https://example.com/subvariant",
                    "GET",
                    "subvariant",
                    "test subvariant",
                    "shuttle",
                )
                .subvariant_suggestions_url(
                    "https://example.com/suggestions-subvariant",
                    "GET",
                    "suggest-subvariant",
                    "sugg test subvariant",
                    "subvariant",
                    true,
                )
                .subvariant_trending_url(
                    "https://example.com/trending-subvariant",
                    "GET",
                    "trend-subvariant",
                    "trend test subvariant",
                    "subtrend",
                )
                .subvariant_search_form_url(
                    "https://example.com/search-form-subvariant",
                    "GET",
                    "search-form-subvariant",
                    "search form subvariant",
                )
                .subvariant_visual_search_url(
                    "https://example.com/visual-search-subvariant",
                    "visual-search-subvariant-name",
                    "visual-search-subvariant-value",
                    "url_subvariant",
                    "Visual Search Subvariant",
                    "2097-03-03",
                )
                .build()
        );
    }

    #[test]
    fn test_merges_subvariant_and_uses_locale_specific_visual_search_display_name() {
        let result = SearchEngineDefinition::from_configuration_details(
            &SearchUserEnvironment {
                locale: "en-GB".into(),
                ..Default::default()
            },
            "test",
            Lazy::force(&JSON_ENGINE_BASE).clone(),
            &JSON_ENGINE_VARIANT,
            &Some(JSON_ENGINE_SUBVARIANT.clone()),
        );

        assert_eq!(
            result,
            ExpectedEngineFromJSONBase::new("test", "Test")
                .variant_is_new_until("2063-04-05")
                .variant_optional(true)
                .subvariant_partner_code("trek2")
                .subvariant_telemetry_suffix("star2")
                .subvariant_search_url(
                    "https://example.com/subvariant",
                    "GET",
                    "subvariant",
                    "test subvariant",
                    "shuttle",
                )
                .subvariant_suggestions_url(
                    "https://example.com/suggestions-subvariant",
                    "GET",
                    "suggest-subvariant",
                    "sugg test subvariant",
                    "subvariant",
                    true,
                )
                .subvariant_trending_url(
                    "https://example.com/trending-subvariant",
                    "GET",
                    "trend-subvariant",
                    "trend test subvariant",
                    "subtrend",
                )
                .subvariant_search_form_url(
                    "https://example.com/search-form-subvariant",
                    "GET",
                    "search-form-subvariant",
                    "search form subvariant",
                )
                .subvariant_visual_search_url(
                    "https://example.com/visual-search-subvariant",
                    "visual-search-subvariant-name",
                    "visual-search-subvariant-value",
                    "url_subvariant",
                    // locale-specific display name is the key difference here
                    "Visual Search Subvariant en-GB",
                    "2097-03-03",
                )
                .build()
        );
    }
}
