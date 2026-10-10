use crate::{
    JSONEngineBase, JSONEngineBaseV3, JSONEngineMethod, JSONEngineUrl, JSONEngineUrls,
    JSONEngineVariant, JSONEngineVariantV3, JSONVariantEnvironment, SearchEngineClassification,
    SearchEngineClassificationV3, SearchEngineDefinition, SearchEngineDefinitionV3,
    SearchEnginePartnerDetails, SearchEngineUrl, SearchEngineUrls, SearchUrlParam,
};
use serde_json::{json, Value};
use std::collections::HashMap;

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
pub struct EngineRecordV3 {
    record_type: String,
    identifier: String,
    name: String,
    classification: String,
    urls: Value,
    variants: Vec<Value>,
    partner: Option<HashMap<String, SearchEnginePartnerDetails>>,

    // Remote Settings metadata
    id: Option<String>,
    schema: Option<i64>,
    last_modified: Option<i64>,
}

#[cfg(test)]
impl EngineRecordV3 {
    pub fn full(identifier: &str, name: &str) -> Self {
        Self {
            record_type: "engine".to_string(),
            identifier: identifier.to_string(),
            name: name.to_string(),
            classification: "general".to_string(),
            partner: Some(HashMap::from([(
                "default".to_string(),
                SearchEnginePartnerDetails {
                    code: Some("partner_code".to_string()),
                    telemetry_suffix: Some("telemetry_suffix".to_string()),
                },
            )])),
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
            partner: None,
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
                "partner": self.partner,
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

    pub fn add_variant(mut self, vb: VariantV3) -> Self {
        self.variants.push(vb.build());
        self
    }

    pub fn override_variants(mut self, vb: VariantV3) -> Self {
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

    pub fn classification(mut self, classification: &str) -> Self {
        self.classification = classification.to_string();
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
pub struct VariantV3 {
    env: Value,
    urls: Option<Value>,
    partner: Option<HashMap<String, SearchEnginePartnerDetails>>,
    optional: Option<bool>,
    subvariants: Vec<Value>,
}

#[cfg(test)]
impl VariantV3 {
    pub fn new() -> Self {
        Self {
            env: json!({}),
            urls: None,
            partner: None,
            optional: None,
            subvariants: vec![],
        }
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

    pub fn partner(mut self, details: &HashMap<String, SearchEnginePartnerDetails>) -> Self {
        self.partner = Some(details.clone());
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

    pub fn add_subvariant(mut self, sv: SubVariantV3) -> Self {
        self.subvariants.push(sv.build());
        self
    }

    pub fn build(self) -> Value {
        let mut variant = json!({ "environment": self.env });

        if let Some(urls) = self.urls {
            variant["urls"] = urls;
        }
        if let Some(partner) = self.partner {
            variant["partner"] = json!(partner);
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
pub struct SubVariantV3 {
    env: Value,
    urls: Option<Value>,
    partner: Option<HashMap<String, SearchEnginePartnerDetails>>,
    optional: Option<bool>,
}

#[cfg(test)]
impl SubVariantV3 {
    pub fn new() -> Self {
        Self {
            env: json!({}),
            urls: None,
            partner: None,
            optional: None,
        }
    }

    pub fn locales(mut self, locales: &[&str]) -> Self {
        self.env["locales"] = json!(locales);
        self
    }

    pub fn partner(mut self, details: &HashMap<String, SearchEnginePartnerDetails>) -> Self {
        self.partner = Some(details.clone());
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
        if let Some(partner) = self.partner {
            subvariant["partner"] = json!(partner);
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

#[cfg(test)]
pub struct ExpectedEngineV3 {
    engine: SearchEngineDefinitionV3,
}

#[cfg(test)]
impl ExpectedEngineV3 {
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

    pub fn partner(self, partner: HashMap<String, SearchEnginePartnerDetails>) -> Self {
        Self {
            engine: SearchEngineDefinitionV3 {
                partner: Some(partner),
                ..self.engine
            },
        }
    }

    pub fn optional(self, value: bool) -> Self {
        Self {
            engine: SearchEngineDefinitionV3 {
                optional: value,
                ..self.engine
            },
        }
    }

    pub fn search_method(self, method: &str) -> Self {
        Self {
            engine: SearchEngineDefinitionV3 {
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        method: method.to_string(),
                        ..self.engine.urls.search
                    },
                    ..self.engine.urls
                },
                ..self.engine
            },
        }
    }

    pub fn search_params(self, params: Vec<SearchUrlParam>) -> Self {
        Self {
            engine: SearchEngineDefinitionV3 {
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        params,
                        ..self.engine.urls.search
                    },
                    ..self.engine.urls
                },
                ..self.engine
            },
        }
    }

    pub fn classification(mut self, classification: SearchEngineClassificationV3) -> Self {
        self.engine.classification = classification;
        self
    }

    pub fn build(self) -> SearchEngineDefinitionV3 {
        self.engine
    }

    fn expected_full_engine(identifier: &str, name: &str) -> SearchEngineDefinitionV3 {
        SearchEngineDefinitionV3 {
            charset: "UTF-8".to_string(),
            classification: SearchEngineClassificationV3::General,
            identifier: identifier.to_string(),
            name: name.to_string(),
            optional: false,
            order_hint: None,
            partner: Some(HashMap::from([(
                "default".to_string(),
                SearchEnginePartnerDetails {
                    code: Some("partner_code".to_string()),
                    telemetry_suffix: Some("telemetry_suffix".to_string()),
                },
            )])),
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

    fn expected_minimal_engine(identifier: &str, name: &str) -> SearchEngineDefinitionV3 {
        SearchEngineDefinitionV3 {
            aliases: Vec::new(),
            charset: "UTF-8".to_string(),
            classification: SearchEngineClassificationV3::General,
            identifier: identifier.to_string(),
            name: name.to_string(),
            optional: false,
            order_hint: None,
            partner: None,
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
            is_new_until: None,
        }
    }
}

#[cfg(test)]
use once_cell::sync::Lazy;

#[cfg(test)]
pub static JSON_ENGINE_BASE: Lazy<JSONEngineBase> = Lazy::new(|| JSONEngineBase {
    aliases: Some(vec!["foo".to_string(), "bar".to_string()]),
    charset: Some("ISO-8859-15".to_string()),
    classification: SearchEngineClassification::Unknown,
    name: "Test".to_string(),
    partner_code: Some("firefox".to_string()),
    urls: JSONEngineUrls {
        search: Some(JSONEngineUrl {
            base: Some("https://example.com".to_string()),
            method: Some(JSONEngineMethod::Post),
            params: Some(vec![
                SearchUrlParam {
                    name: "param".to_string(),
                    value: Some("test param".to_string()),
                    enterprise_value: None,
                    experiment_config: None,
                },
                SearchUrlParam {
                    name: "enterprise-name".to_string(),
                    value: None,
                    enterprise_value: Some("enterprise-value".to_string()),
                    experiment_config: None,
                },
            ]),
            search_term_param_name: Some("baz".to_string()),
            ..Default::default()
        }),
        suggestions: Some(JSONEngineUrl {
            base: Some("https://example.com/suggestions".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "suggest-name".to_string(),
                value: None,
                enterprise_value: None,
                experiment_config: Some("suggest-experiment-value".to_string()),
            }]),
            search_term_param_name: Some("suggest".to_string()),
            ..Default::default()
        }),
        trending: Some(JSONEngineUrl {
            base: Some("https://example.com/trending".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "trend-name".to_string(),
                value: Some("trend-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            ..Default::default()
        }),
        search_form: Some(JSONEngineUrl {
            base: Some("https://example.com/search_form".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "search-form-name".to_string(),
                value: Some("search-form-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            ..Default::default()
        }),
        visual_search: Some(JSONEngineUrl {
            base: Some("https://example.com/visual_search".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "visual-search-name".to_string(),
                value: Some("visual-search-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("url".to_string()),
            display_name_map: Some(HashMap::from([
                // Default display name
                ("default".to_string(), "Visual Search".to_string()),
                // en-GB locale with unique display name
                ("en-GB".to_string(), "Visual Search en-GB".to_string()),
            ])),
            is_new_until: Some("2095-01-01".to_string()),
            exclude_partner_code_from_telemetry: true,
            accepted_content_types: Some(vec!["image/gif".to_string(), "image/jpeg".to_string()]),
        }),
    },
});

#[cfg(test)]
pub static JSON_ENGINE_VARIANT: Lazy<JSONEngineVariant> = Lazy::new(|| JSONEngineVariant {
    environment: JSONVariantEnvironment {
        all_regions_and_locales: true,
        ..Default::default()
    },
    is_new_until: Some("2063-04-05".to_string()),
    optional: true,
    partner_code: Some("trek".to_string()),
    telemetry_suffix: Some("star".to_string()),
    urls: Some(JSONEngineUrls {
        search: Some(JSONEngineUrl {
            base: Some("https://example.com/variant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "variant".to_string(),
                value: Some("test variant".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("ship".to_string()),
            ..Default::default()
        }),
        suggestions: Some(JSONEngineUrl {
            base: Some("https://example.com/suggestions-variant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "suggest-variant".to_string(),
                value: Some("sugg test variant".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("variant".to_string()),
            ..Default::default()
        }),
        trending: Some(JSONEngineUrl {
            base: Some("https://example.com/trending-variant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "trend-variant".to_string(),
                value: Some("trend test variant".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("trend".to_string()),
            exclude_partner_code_from_telemetry: true,
            ..Default::default()
        }),
        search_form: Some(JSONEngineUrl {
            base: Some("https://example.com/search_form".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "search-form-name".to_string(),
                value: Some("search-form-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            ..Default::default()
        }),
        visual_search: Some(JSONEngineUrl {
            base: Some("https://example.com/visual-search-variant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "visual-search-variant-name".to_string(),
                value: Some("visual-search-variant-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("url_variant".to_string()),
            display_name_map: Some(HashMap::from([
                ("default".to_string(), "Visual Search Variant".to_string()),
                (
                    "en-GB".to_string(),
                    "Visual Search Variant en-GB".to_string(),
                ),
            ])),
            is_new_until: Some("2096-02-02".to_string()),
            accepted_content_types: Some(vec!["image/png".to_string(), "image/jpeg".to_string()]),
            ..Default::default()
        }),
    }),
    sub_variants: vec![],
});

#[cfg(test)]
pub static JSON_ENGINE_SUBVARIANT: Lazy<JSONEngineVariant> = Lazy::new(|| JSONEngineVariant {
    environment: JSONVariantEnvironment {
        all_regions_and_locales: true,
        ..Default::default()
    },
    is_new_until: Some("2063-04-05".to_string()),
    optional: true,
    partner_code: Some("trek2".to_string()),
    telemetry_suffix: Some("star2".to_string()),
    urls: Some(JSONEngineUrls {
        search: Some(JSONEngineUrl {
            base: Some("https://example.com/subvariant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "subvariant".to_string(),
                value: Some("test subvariant".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("shuttle".to_string()),
            ..Default::default()
        }),
        suggestions: Some(JSONEngineUrl {
            base: Some("https://example.com/suggestions-subvariant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "suggest-subvariant".to_string(),
                value: Some("sugg test subvariant".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("subvariant".to_string()),
            exclude_partner_code_from_telemetry: true,
            ..Default::default()
        }),
        trending: Some(JSONEngineUrl {
            base: Some("https://example.com/trending-subvariant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "trend-subvariant".to_string(),
                value: Some("trend test subvariant".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("subtrend".to_string()),
            ..Default::default()
        }),
        search_form: Some(JSONEngineUrl {
            base: Some("https://example.com/search-form-subvariant".to_string()),
            method: Some(crate::JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "search-form-subvariant".to_string(),
                value: Some("search form subvariant".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            ..Default::default()
        }),
        visual_search: Some(JSONEngineUrl {
            base: Some("https://example.com/visual-search-subvariant".to_string()),
            method: Some(JSONEngineMethod::Get),
            params: Some(vec![SearchUrlParam {
                name: "visual-search-subvariant-name".to_string(),
                value: Some("visual-search-subvariant-value".to_string()),
                enterprise_value: None,
                experiment_config: None,
            }]),
            search_term_param_name: Some("url_subvariant".to_string()),
            display_name_map: Some(HashMap::from([
                (
                    "default".to_string(),
                    "Visual Search Subvariant".to_string(),
                ),
                // en-GB locale with unique display name
                (
                    "en-GB".to_string(),
                    "Visual Search Subvariant en-GB".to_string(),
                ),
            ])),
            is_new_until: Some("2097-03-03".to_string()),
            accepted_content_types: Some(vec!["image/jpeg".to_string(), "image/webp".to_string()]),
            ..Default::default()
        }),
    }),
    sub_variants: vec![],
});

#[cfg(test)]
pub fn partner_map(
    entries: &[(&str, &str, Option<&str>)],
) -> HashMap<String, SearchEnginePartnerDetails> {
    entries
        .iter()
        .map(|(access_point, partner_code, telemetry_suffix)| {
            (
                access_point.to_string(),
                SearchEnginePartnerDetails {
                    code: Some(partner_code.to_string()),
                    telemetry_suffix: telemetry_suffix.map(|s| s.to_string()),
                },
            )
        })
        .collect()
}

#[cfg(test)]
impl From<SearchEngineClassification> for SearchEngineClassificationV3 {
    fn from(classification: SearchEngineClassification) -> Self {
        match classification {
            SearchEngineClassification::General => SearchEngineClassificationV3::General,
            SearchEngineClassification::Unknown => SearchEngineClassificationV3::Unknown,
        }
    }
}

#[cfg(test)]
pub static JSON_ENGINE_BASE_V3: Lazy<JSONEngineBaseV3> = Lazy::new(|| JSONEngineBaseV3 {
    aliases: JSON_ENGINE_BASE.aliases.clone(),
    charset: JSON_ENGINE_BASE.charset.clone(),
    classification: JSON_ENGINE_BASE.classification.clone().into(),
    name: JSON_ENGINE_BASE.name.clone(),
    partner: Some(partner_map(&[
        ("default", "firefox", None),
        ("newtab", "firefox-newtab", Some("nt")),
    ])),
    urls: JSON_ENGINE_BASE.urls.clone(),
});

#[cfg(test)]
pub static JSON_ENGINE_VARIANT_V3: Lazy<JSONEngineVariantV3> = Lazy::new(|| JSONEngineVariantV3 {
    environment: JSONVariantEnvironment {
        all_regions_and_locales: JSON_ENGINE_VARIANT.environment.all_regions_and_locales,
        ..Default::default()
    },
    is_new_until: JSON_ENGINE_VARIANT.is_new_until.clone(),
    optional: JSON_ENGINE_VARIANT.optional,
    partner: Some(partner_map(&[("default", "trek", Some("star"))])),
    urls: JSON_ENGINE_VARIANT.urls.clone(),
    sub_variants: vec![],
});

#[cfg(test)]
pub static JSON_ENGINE_SUBVARIANT_V3: Lazy<JSONEngineVariantV3> =
    Lazy::new(|| JSONEngineVariantV3 {
        environment: JSONVariantEnvironment {
            all_regions_and_locales: JSON_ENGINE_SUBVARIANT.environment.all_regions_and_locales,
            ..Default::default()
        },
        is_new_until: JSON_ENGINE_SUBVARIANT.is_new_until.clone(),
        optional: JSON_ENGINE_SUBVARIANT.optional,
        partner: Some(partner_map(&[("default", "trek2", Some("star2"))])),
        urls: JSON_ENGINE_SUBVARIANT.urls.clone(),
        sub_variants: vec![],
    });

#[cfg(test)]
pub struct ExpectedEngineFromJSONBase {
    engine: SearchEngineDefinition,
}

#[cfg(test)]
impl ExpectedEngineFromJSONBase {
    pub fn new(identifier: &str, name: &str) -> Self {
        Self {
            engine: SearchEngineDefinition {
                aliases: vec!["foo".to_string(), "bar".to_string()],
                charset: "ISO-8859-15".to_string(),
                classification: SearchEngineClassification::Unknown,
                identifier: identifier.to_string(),
                is_new_until: None,
                partner_code: "firefox".to_string(),
                name: name.to_string(),
                optional: false,
                order_hint: None,
                telemetry_suffix: String::new(),
                urls: SearchEngineUrls {
                    search: SearchEngineUrl {
                        base: "https://example.com".to_string(),
                        method: "POST".to_string(),
                        params: vec![
                            SearchUrlParam {
                                name: "param".to_string(),
                                value: Some("test param".to_string()),
                                enterprise_value: None,
                                experiment_config: None,
                            },
                            SearchUrlParam {
                                name: "enterprise-name".to_string(),
                                value: None,
                                enterprise_value: Some("enterprise-value".to_string()),
                                experiment_config: None,
                            },
                        ],
                        search_term_param_name: Some("baz".to_string()),
                        ..Default::default()
                    },
                    suggestions: Some(SearchEngineUrl {
                        base: "https://example.com/suggestions".to_string(),
                        method: "GET".to_string(),
                        params: vec![SearchUrlParam {
                            name: "suggest-name".to_string(),
                            value: None,
                            enterprise_value: None,
                            experiment_config: Some("suggest-experiment-value".to_string()),
                        }],
                        search_term_param_name: Some("suggest".to_string()),
                        ..Default::default()
                    }),
                    trending: Some(SearchEngineUrl {
                        base: "https://example.com/trending".to_string(),
                        method: "GET".to_string(),
                        params: vec![SearchUrlParam {
                            name: "trend-name".to_string(),
                            value: Some("trend-value".to_string()),
                            enterprise_value: None,
                            experiment_config: None,
                        }],
                        ..Default::default()
                    }),
                    search_form: Some(SearchEngineUrl {
                        base: "https://example.com/search_form".to_string(),
                        method: "GET".to_string(),
                        params: vec![SearchUrlParam {
                            name: "search-form-name".to_string(),
                            value: Some("search-form-value".to_string()),
                            enterprise_value: None,
                            experiment_config: None,
                        }],
                        ..Default::default()
                    }),
                    visual_search: Some(SearchEngineUrl {
                        base: "https://example.com/visual_search".to_string(),
                        method: "GET".to_string(),
                        params: vec![SearchUrlParam {
                            name: "visual-search-name".to_string(),
                            value: Some("visual-search-value".to_string()),
                            enterprise_value: None,
                            experiment_config: None,
                        }],
                        search_term_param_name: Some("url".to_string()),
                        display_name: Some("Visual Search".to_string()),
                        is_new_until: Some("2095-01-01".to_string()),
                        exclude_partner_code_from_telemetry: true,
                        accepted_content_types: Some(vec![
                            "image/gif".to_string(),
                            "image/jpeg".to_string(),
                        ]),
                    }),
                },
                click_url: None,
            },
        }
    }
    pub fn variant_is_new_until(mut self, date: &str) -> Self {
        self.engine.is_new_until = Some(date.to_string());
        self
    }

    pub fn variant_optional(mut self, optional: bool) -> Self {
        self.engine.optional = optional;
        self
    }

    pub fn variant_partner_code(mut self, partner_code: &str) -> Self {
        self.engine.partner_code = partner_code.to_string();
        self
    }

    pub fn variant_telemetry_suffix(mut self, suffix: &str) -> Self {
        self.engine.telemetry_suffix = suffix.to_string();
        self
    }

    pub fn visual_search_display_name(mut self, display_name: &str) -> Self {
        let visual = self
            .engine
            .urls
            .visual_search
            .as_mut()
            .expect("Expected base engine to include visual_search");

        visual.display_name = Some(display_name.to_string());
        self
    }

    pub fn variant_search_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
    ) -> Self {
        self.engine.urls.search = SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            ..Default::default()
        };
        self
    }

    pub fn variant_suggestions_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
    ) -> Self {
        self.engine.urls.suggestions = Some(SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            ..Default::default()
        });
        self
    }

    pub fn variant_trending_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
        exclude_partner_code_from_telemetry: bool,
    ) -> Self {
        self.engine.urls.trending = Some(SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            exclude_partner_code_from_telemetry,
            ..Default::default()
        });
        self
    }

    pub fn variant_search_form_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
    ) -> Self {
        self.engine.urls.search_form = Some(SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            ..Default::default()
        });
        self
    }

    pub fn variant_visual_search_url(
        mut self,
        base: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
        display_name: &str,
        is_new_until: &str,
    ) -> Self {
        self.engine.urls.visual_search = Some(SearchEngineUrl {
            base: base.to_string(),
            method: "GET".to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            display_name: Some(display_name.to_string()),
            is_new_until: Some(is_new_until.to_string()),
            accepted_content_types: Some(vec!["image/png".to_string(), "image/jpeg".to_string()]),
            exclude_partner_code_from_telemetry: false,
        });
        self
    }

    pub fn subvariant_partner_code(mut self, partner_code: &str) -> Self {
        self.engine.partner_code = partner_code.to_string();
        self
    }

    pub fn subvariant_telemetry_suffix(mut self, suffix: &str) -> Self {
        self.engine.telemetry_suffix = suffix.to_string();
        self
    }

    pub fn subvariant_search_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
    ) -> Self {
        self.engine.urls.search = SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            ..Default::default()
        };
        self
    }

    pub fn subvariant_suggestions_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
        exclude_partner_code_from_telemetry: bool,
    ) -> Self {
        self.engine.urls.suggestions = Some(SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            exclude_partner_code_from_telemetry,
            ..Default::default()
        });
        self
    }

    pub fn subvariant_trending_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
    ) -> Self {
        self.engine.urls.trending = Some(SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            ..Default::default()
        });
        self
    }

    pub fn subvariant_search_form_url(
        mut self,
        base: &str,
        method: &str,
        param_name: &str,
        param_value: &str,
    ) -> Self {
        self.engine.urls.search_form = Some(SearchEngineUrl {
            base: base.to_string(),
            method: method.to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            ..Default::default()
        });
        self
    }

    pub fn subvariant_visual_search_url(
        mut self,
        base: &str,
        param_name: &str,
        param_value: &str,
        search_term_param_name: &str,
        display_name: &str,
        is_new_until: &str,
    ) -> Self {
        self.engine.urls.visual_search = Some(SearchEngineUrl {
            base: base.to_string(),
            method: "GET".to_string(),
            params: vec![SearchUrlParam {
                name: param_name.to_string(),
                value: Some(param_value.to_string()),
                enterprise_value: None,
                experiment_config: None,
            }],
            search_term_param_name: Some(search_term_param_name.to_string()),
            display_name: Some(display_name.to_string()),
            is_new_until: Some(is_new_until.to_string()),
            exclude_partner_code_from_telemetry: false,
            accepted_content_types: Some(vec!["image/jpeg".to_string(), "image/webp".to_string()]),
        });
        self
    }

    pub fn build(self) -> SearchEngineDefinition {
        self.engine
    }

    /// Builds the equivalent v3 engine definition. The v1 partner code and
    /// telemetry suffix are replaced by the supplied partner details.
    pub fn build_v3(
        self,
        partner: Option<HashMap<String, SearchEnginePartnerDetails>>,
    ) -> SearchEngineDefinitionV3 {
        SearchEngineDefinitionV3 {
            aliases: self.engine.aliases,
            charset: self.engine.charset,
            classification: self.engine.classification.into(),
            identifier: self.engine.identifier,
            is_new_until: self.engine.is_new_until,
            name: self.engine.name,
            optional: self.engine.optional,
            order_hint: self.engine.order_hint,
            partner,
            urls: self.engine.urls,
        }
    }
}
