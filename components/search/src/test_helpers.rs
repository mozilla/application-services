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
    partner_code: String,

    // Remote Settings metadata
    id: Option<String>,
    schema: Option<i64>,
    last_modified: Option<i64>,
}

#[cfg(test)]
impl EngineRecord {
    pub fn full(identifier: &str, name: &str) -> Self {
        Self {
            record_type: "engine".to_string(),
            identifier: identifier.to_string(),
            name: name.to_string(),
            classification: "general".to_string(),
            partner_code: "partner-code".to_string(),
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
            // Remote Settings metadata
            id: None,
            schema: None,
            last_modified: None,
        }
    }
    pub fn minimal(identifier: &str, name: &str) -> Self {
        Self {
            record_type: "engine".to_string(),
            identifier: identifier.to_string(),
            name: name.to_string(),
            classification: "general".to_string(),
            partner_code: "partner-code".to_string(),
            urls: json!({
                "search": {
                    "base": "https://example.com",
                    "method": "GET",
                    "searchTermParamName": "search",
                },
            }),
            variants: vec![json!({
                "environment": {
                    "allRegionsAndLocales": true,
                }
            })],
            // Remote Settings metadata
            id: None,
            schema: None,
            last_modified: None,
        }
    }

    pub fn build(self) -> Value {
        let mut record = json!({
            "recordType": self.record_type,
            "identifier": self.identifier,
            "base": {
                "name": self.name,
                "classification": self.classification,
                "partnerCode": self.partner_code,
                "urls": self.urls,
            },
            "variants": self.variants,
        });

        if let Some(id) = self.id {
            record["id"] = json!(id);
        }
        if let Some(schema) = self.schema {
            record["schema"] = json!(schema);
        }
        if let Some(last_modified) = self.last_modified {
            record["last_modified"] = json!(last_modified);
        }
        record
    }

    pub fn add_variant(mut self, vb: Variant) -> Self {
        self.variants.push(vb.build());
        self
    }

    pub fn override_variants(mut self, vb: Variant) -> Self {
        self.variants = vec![vb.build()];
        self
    }

    // Remote Settings metadata
    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    pub fn schema(mut self, schema: i64) -> Self {
        self.schema = Some(schema);
        self
    }

    pub fn last_modified(mut self, last_modified: i64) -> Self {
        self.last_modified = Some(last_modified);
        self
    }
}

#[cfg(test)]
pub struct Variant {
    env: Value,
    urls: Option<Value>,
    partner_code: Option<String>,
    telemetry_suffix: Option<String>,
    optional: Option<bool>,
    subvariants: Vec<Value>,
}

#[cfg(test)]
impl Variant {
    pub fn new() -> Self {
        Self {
            env: json!({}),
            urls: None,
            partner_code: None,
            telemetry_suffix: None,
            optional: None,
            subvariants: vec![],
        }
    }

    pub fn all_regions_and_locales(mut self) -> Self {
        self.env["allRegionsAndLocales"] = serde_json::Value::Bool(true);
        self
    }

    pub fn regions(mut self, regions: &[&str]) -> Self {
        self.env["regions"] = json!(regions);
        self
    }

    pub fn locales(mut self, locales: &[&str]) -> Self {
        self.env["locales"] = json!(locales);
        self
    }

    pub fn applications(mut self, applications: &[&str]) -> Self {
        self.env["applications"] = json!(applications);
        self
    }

    pub fn distributions(mut self, distributions: &[&str]) -> Self {
        self.env["distributions"] = json!(distributions);
        self
    }

    pub fn partner_code(mut self, code: &str) -> Self {
        self.partner_code = Some(code.to_string());
        self
    }

    pub fn telemetry_suffix(mut self, suffix: &str) -> Self {
        self.telemetry_suffix = Some(suffix.to_string());
        self
    }

    pub fn optional(mut self, value: bool) -> Self {
        self.optional = Some(value);
        self
    }

    pub fn urls(mut self, urls: Value) -> Self {
        self.urls = Some(urls);
        self
    }

    pub fn add_subvariant(mut self, sv: SubVariant) -> Self {
        self.subvariants.push(sv.build());
        self
    }

    pub fn build(self) -> Value {
        let mut variant = json!({ "environment": self.env });

        if let Some(urls) = self.urls {
            variant["urls"] = urls;
        }
        if let Some(partner_code) = self.partner_code {
            variant["partnerCode"] = json!(partner_code);
        }
        if let Some(telemetry_suffix) = self.telemetry_suffix {
            variant["telemetrySuffix"] = json!(telemetry_suffix);
        }
        if let Some(optional) = self.optional {
            variant["optional"] = json!(optional);
        }
        if !self.subvariants.is_empty() {
            variant["subVariants"] = json!(self.subvariants);
        }

        variant
    }
}

#[cfg(test)]
pub struct SubVariant {
    env: Value,
    urls: Option<Value>,
    partner_code: Option<String>,
    telemetry_suffix: Option<String>,
    optional: Option<bool>,
}

#[cfg(test)]
impl SubVariant {
    pub fn new() -> Self {
        Self {
            env: json!({}),
            urls: None,
            partner_code: None,
            telemetry_suffix: None,
            optional: None,
        }
    }

    pub fn locales(mut self, locales: &[&str]) -> Self {
        self.env["locales"] = json!(locales);
        self
    }

    pub fn partner_code(mut self, code: &str) -> Self {
        self.partner_code = Some(code.to_string());
        self
    }

    pub fn telemetry_suffix(mut self, suffix: &str) -> Self {
        self.telemetry_suffix = Some(suffix.to_string());
        self
    }

    pub fn urls(mut self, urls: Value) -> Self {
        self.urls = Some(urls);
        self
    }

    pub fn build(self) -> Value {
        let mut subvariant = json!({ "environment": self.env });
        if let Some(urls) = self.urls {
            subvariant["urls"] = urls;
        }
        if let Some(partner_code) = self.partner_code {
            subvariant["partnerCode"] = json!(partner_code);
        }
        if let Some(telemetry_suffix) = self.telemetry_suffix {
            subvariant["telemetrySuffix"] = json!(telemetry_suffix);
        }
        if let Some(optional) = self.optional {
            subvariant["optional"] = json!(optional);
        }
        subvariant
    }
}

#[cfg(test)]
pub fn overrides_engine() -> Value {
    json!({
      "identifier": "overrides-engine",
      "name": "Overrides Engine",
      "partnerCode": "overrides-partner-code",
      "clickUrl": "https://example.com/click-url",
      "telemetrySuffix": "overrides-telemetry-suffix",
      "urls": {
        "search": {
          "base": "https://example.com/search-overrides",
          "method": "GET",
            "params": [{
              "name": "overrides-name",
              "value": "overrides-value",
            }],
        }
      }
    })
}

#[cfg(test)]
pub struct ExpectedEngine {
    engine: SearchEngineDefinition,
}

#[cfg(test)]
impl ExpectedEngine {
    pub fn full(identifier: &str, name: &str) -> Self {
        Self {
            engine: Self::expected_full_engine(identifier, name),
        }
    }

    pub fn minimal(identifier: &str, name: &str) -> Self {
        Self {
            engine: Self::expected_minimal_engine(identifier, name),
        }
    }

    pub fn partner_code(mut self, code: &str) -> Self {
        self.engine.partner_code = code.to_string();
        self
    }

    pub fn telemetry_suffix(mut self, suffix: &str) -> Self {
        self.engine.telemetry_suffix = suffix.to_string();
        self
    }

    pub fn click_url(mut self, url: &str) -> Self {
        self.engine.click_url = Some(url.to_string());
        self
    }

    pub fn optional(mut self, value: bool) -> Self {
        self.engine.optional = value;
        self
    }

    pub fn search_method(mut self, method: &str) -> Self {
        self.engine.urls.search.method = method.to_string();
        self
    }

    pub fn search_base(mut self, base: &str) -> Self {
        self.engine.urls.search.base = base.to_string();
        self
    }

    pub fn search_term_param_name(mut self, param: &str) -> Self {
        self.engine.urls.search.search_term_param_name = Some(param.to_string());
        self
    }

    pub fn search_params(mut self, params: Vec<SearchUrlParam>) -> Self {
        self.engine.urls.search.params = params;
        self
    }

    pub fn build(self) -> SearchEngineDefinition {
        self.engine
    }

    fn expected_full_engine(identifier: &str, name: &str) -> SearchEngineDefinition {
        SearchEngineDefinition {
            charset: "UTF-8".to_string(),
            classification: SearchEngineClassification::General,
            identifier: identifier.to_string(),
            name: name.to_string(),
            partner_code: "partner-code".to_string(),
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

    fn expected_minimal_engine(identifier: &str, name: &str) -> SearchEngineDefinition {
        SearchEngineDefinition {
            aliases: Vec::new(),
            charset: "UTF-8".to_string(),
            classification: SearchEngineClassification::General,
            identifier: identifier.to_string(),
            is_new_until: None,
            name: name.to_string(),
            optional: false,
            order_hint: None,
            partner_code: "partner-code".to_string(),
            telemetry_suffix: String::new(),
            urls: SearchEngineUrls {
                search: SearchEngineUrl {
                    base: "https://example.com".to_string(),
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
}
