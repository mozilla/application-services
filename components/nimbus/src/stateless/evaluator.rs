/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{evaluator::split_locale, stateless::matcher::AppContext};
use serde_derive::*;
use serde_json::Map;
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TargetingAttributes {
    #[serde(flatten)]
    pub app_context: AppContext,
    #[serde(flatten)]
    pub request_context: Map<String, Value>,
    pub language: Option<String>,
    pub region: Option<String>,
}

impl TargetingAttributes {
    pub fn new(app_context: AppContext, request_context: Map<String, Value>) -> Self {
        let (language, region) = match request_context
            .get("locale")
            .unwrap_or(&Value::Null)
            .as_str()
        {
            Some(locale) => split_locale(locale.to_string()),
            _ => (None, None),
        };

        Self {
            app_context,
            request_context,
            language,
            region,
        }
    }
}
