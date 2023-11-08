/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{evaluator::split_locale, stateful::matcher::AppContext};
use serde_derive::*;
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TargetingAttributes {
    #[serde(flatten)]
    pub app_context: AppContext,
    pub language: Option<String>,
    pub region: Option<String>,
    pub is_already_enrolled: bool,
    pub days_since_install: Option<i32>,
    pub days_since_update: Option<i32>,
    pub active_experiments: HashSet<String>,
    pub enrollments: HashSet<String>,
    pub enrollments_map: HashMap<String, String>,
}

#[cfg(feature = "stateful")]
impl From<AppContext> for TargetingAttributes {
    fn from(app_context: AppContext) -> Self {
        let (language, region) = app_context
            .locale
            .clone()
            .map(split_locale)
            .unwrap_or_else(|| (None, None));

        Self {
            app_context,
            language,
            region,
            ..Default::default()
        }
    }
}
