/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use bso_record::Sync15Record;
use std::collections::HashMap;

// Known record formats.

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRecord {
    pub id: String,
    pub hostname: Option<String>,

    // rename_all = "camelCase" by default will do formSubmitUrl, but we can just
    // override this one field.
    #[serde(rename = "formSubmitURL")]
    pub form_submit_url: Option<String>,

    pub http_realm: Option<String>,

    #[serde(default = "String::new")]
    pub username: String,

    pub password: String,

    #[serde(default = "String::new")]
    pub username_field: String,

    #[serde(default = "String::new")]
    pub password_field: String,

    pub time_created: i64,
    pub time_password_changed: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_last_used: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub times_used: Option<i64>,
}

impl Sync15Record for PasswordRecord {
    fn collection_tag() -> &'static str { "passwords" }
    fn record_id(&self) -> &str { &self.id }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalEngine {
    pub version: usize,
    #[serde(rename = "syncID")]
    pub sync_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalRecord {
    #[serde(rename = "syncID")]
    pub sync_id: String,
    #[serde(rename = "storageVersion")]
    pub storage_version: usize,
    pub engines: HashMap<String, MetaGlobalEngine>,
    pub declined: Vec<String>,
}

impl Sync15Record for MetaGlobalRecord {
    fn collection_tag() -> &'static str { "meta" }
    fn record_id(&self) -> &str { "global" }
}
