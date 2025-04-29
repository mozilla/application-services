use crate::{
    SearchEngineClassification, SearchEngineDefinition, SearchEngineUrl, SearchEngineUrls,
    SearchUrlParam,
};
use serde_json::{json, Value};

#[cfg(test)]
pub struct EngineRecord {
    record_type: String,
    identifier: String,
    name: String,
    classification: String,
    urls: Value,
    variants: Vec<Value>,
}

#[cfg(test)]
impl EngineRecord {
    pub fn full() -> Self {
        Self {
            record_type: "engine".to_string(),
            identifier: "test1".to_string(),
            name: "Test 1".to_string(),
            classification: "general".to_string(),
            urls: json!({
                "search": {
                    "base": "https://example.com",
                    "method": "GET",
                    "params": [{
                        "name": "search-name",
                        "enterpriseValue": "enterprise-value",
                    }],
                    "searchTermParamName": "q",
                },
                "suggestions": {
                    "base": "https://example.com/suggestions",
                    "method": "POST",
                    "params": [{
                        "name": "suggestion-name",
                        "value": "suggestion-value",
                    }],
                    "searchTermParamName": "suggest",
                },
                "trending": {
                    "base": "https://example.com/trending",
                    "method": "GET",
                    "params": [{
                        "name": "trending-name",
                        "experimentConfig": "trending-experiment-value",
                    }],
                },
                "searchForm": {
                    "base": "https://example.com/search-form",
                    "method": "GET",
                    "params": [{
                        "name": "search-form-name",
                        "value": "search-form-value",
                    }],
                },
                "visualSearch": {
                    "base": "https://example.com/visual-search",
                    "method": "GET",
                    "params": [{
                        "name": "visual-search-name",
                        "value": "visual-search-value",
                    }],
                    "searchTermParamName": "url",
                },
            }),
            variants: vec![json!({
                "environment": {
                    "allRegionsAndLocales": true,
                }
            })],
        }
    }
    pub fn minimal() -> Self {
        Self {
            record_type: "engine".to_string(),
            identifier: "test2".to_string(),
            name: "Test 2".to_string(),
            classification: "unknown".to_string(),
            urls: json!({
                "search": {
                    "base": "https://example.com/2",
                    "method": "GET",
                    "searchTermParamName": "search",
                },
            }),
            variants: vec![json!({
                "environment": {
                    "allRegionsAndLocales": true,
                }
            })],
        }
    }

    pub fn identifier(mut self, id: &str) -> Self {
        self.identifier = id.to_string();
        self
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn search_url(
        mut self,
        base: &str,
        method: &str,
        params: Option<Value>,
        search_term_param_name: &str,
    ) -> Self {
        if let Some(search) = self.urls.get_mut("search").and_then(|v| v.as_object_mut()) {
            search.insert("base".into(), json!(base));
            search.insert("method".into(), json!(method));

            if let Some(p) = params {
                search.insert("params".into(), p);
            }

            search.insert("searchTermParamName".into(), json!(search_term_param_name));
        }

        self
    }

    pub fn build(self) -> Value {
        json!({
            "recordType": self.record_type,
            "identifier": self.identifier,
            "base": {
                "name": self.name,
                "classification": self.classification,
                "urls": self.urls,
            },
            "variants": self.variants,
        })
    }
}

#[cfg(test)]
pub fn expected_full_engine() -> SearchEngineDefinition {
    SearchEngineDefinition {
        charset: "UTF-8".to_string(),
        classification: SearchEngineClassification::General,
        identifier: "test1".to_string(),
        name: "Test 1".to_string(),
        urls: SearchEngineUrls {
            search: SearchEngineUrl {
                base: "https://example.com".to_string(),
                method: "GET".to_string(),
                params: vec![SearchUrlParam {
                    name: "search-name".to_string(),
                    value: None,
                    enterprise_value: Some("enterprise-value".to_string()),
                    experiment_config: None,
                }],
                search_term_param_name: Some("q".to_string()),
                ..Default::default()
            },
            suggestions: Some(SearchEngineUrl {
                base: "https://example.com/suggestions".to_string(),
                method: "POST".to_string(),
                params: vec![SearchUrlParam {
                    name: "suggestion-name".to_string(),
                    value: Some("suggestion-value".to_string()),
                    enterprise_value: None,
                    experiment_config: None,
                }],
                search_term_param_name: Some("suggest".to_string()),
                ..Default::default()
            }),
            trending: Some(SearchEngineUrl {
                base: "https://example.com/trending".to_string(),
                method: "GET".to_string(),
                params: vec![SearchUrlParam {
                    name: "trending-name".to_string(),
                    value: None,
                    enterprise_value: None,
                    experiment_config: Some("trending-experiment-value".to_string()),
                }],
                ..Default::default()
            }),
            search_form: Some(SearchEngineUrl {
                base: "https://example.com/search-form".to_string(),
                method: "GET".to_string(),
                params: vec![SearchUrlParam {
                    name: "search-form-name".to_string(),
                    value: Some("search-form-value".to_string()),
                    experiment_config: None,
                    enterprise_value: None,
                }],
                ..Default::default()
            }),
            visual_search: Some(SearchEngineUrl {
                base: "https://example.com/visual-search".to_string(),
                method: "GET".to_string(),
                params: vec![SearchUrlParam {
                    name: "visual-search-name".to_string(),
                    value: Some("visual-search-value".to_string()),
                    experiment_config: None,
                    enterprise_value: None,
                }],
                search_term_param_name: Some("url".to_string()),
                ..Default::default()
            }),
        },
        ..Default::default()
    }
}

pub fn expected_minimal_engine() -> SearchEngineDefinition {
    SearchEngineDefinition {
        aliases: Vec::new(),
        charset: "UTF-8".to_string(),
        classification: SearchEngineClassification::Unknown,
        identifier: "test2".to_string(),
        is_new_until: None,
        name: "Test 2".to_string(),
        optional: false,
        order_hint: None,
        partner_code: String::new(),
        telemetry_suffix: String::new(),
        urls: SearchEngineUrls {
            search: SearchEngineUrl {
                base: "https://example.com/2".to_string(),
                search_term_param_name: Some("search".to_string()),
                ..Default::default()
            },
            suggestions: None,
            trending: None,
            search_form: None,
            visual_search: None,
        },
        click_url: None,
    }
}
