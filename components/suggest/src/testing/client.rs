/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use remote_settings::Attachment;
use serde_json::json;
use serde_json::Value as JsonValue;

use crate::{
    rs::{
        SuggestRemoteSettingsClient, SuggestRemoteSettingsRecord,
        SuggestRemoteSettingsRecordRequest,
    },
    testing::JsonExt,
    Result,
};

/// Mock remote settings client
///
/// MockRemoteSettingsClient uses the builder pattern for its API: most methods input `self` and
/// return a modified version of it.
pub struct MockRemoteSettingsClient {
    pub records: HashMap<String, Vec<SuggestRemoteSettingsRecord>>,
    pub last_modified_timestamp: u64,
}

impl Default for MockRemoteSettingsClient {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            last_modified_timestamp: 100,
        }
    }
}

impl MockRemoteSettingsClient {
    /// Add a record to the mock data
    ///
    /// A single record typically contains multiple items in the attachment data.  Pass all of them
    /// as the `items` param.
    pub fn with_record(mut self, record_type: &str, record_id: &str, items: JsonValue) -> Self {
        let location = format!("{record_type}-{record_id}.json");
        let records = self.records.entry(record_type.to_string()).or_default();
        records.push(SuggestRemoteSettingsRecord {
            id: record_id.to_string(),
            last_modified: self.last_modified_timestamp,
            deleted: false,
            attachment: Some(Attachment {
                filename: location.clone(),
                mimetype: "application/json".into(),
                hash: "".into(),
                size: 0,
                location,
            }),
            fields: json!({"type": record_type}).into_map(),
            attachment_data: Some(
                serde_json::to_vec(&items).expect("error serializing attachment data"),
            ),
        });
        self
    }

    /// Add a tombstone record
    ///
    /// This is used by remote settings to indicated a deleted record
    pub fn with_tombstone(mut self, record_type: &str, record_id: &str) -> Self {
        let records = self.records.entry(record_type.to_string()).or_default();
        records.push(SuggestRemoteSettingsRecord {
            id: record_id.to_string(),
            last_modified: self.last_modified_timestamp,
            deleted: true,
            attachment: None,
            attachment_data: None,
            fields: json!({}).into_map(),
        });
        self
    }

    /// Add a record for an icon to the mock data
    pub fn with_icon(mut self, icon: MockIcon) -> Self {
        let icon_id = icon.id;
        let record_id = format!("icon-{icon_id}");
        let location = format!("icon-{icon_id}.png");
        let records = self.records.entry("icon".to_string()).or_default();
        records.push(SuggestRemoteSettingsRecord {
            id: record_id.to_string(),
            last_modified: self.last_modified_timestamp,
            deleted: false,
            attachment: Some(Attachment {
                filename: location.clone(),
                mimetype: icon.mimetype.into(),
                hash: "".into(),
                size: 0,
                location,
            }),
            fields: json!({"type": "icon"}).into_map(),
            attachment_data: Some(icon.data.as_bytes().to_vec()),
        });
        self
    }

    /// Add a tombstone record for an icon
    pub fn with_icon_tombstone(self, icon: MockIcon) -> Self {
        self.with_tombstone("icon", &format!("icon-{}", icon.id))
    }
}

pub struct MockIcon {
    pub id: &'static str,
    pub data: &'static str,
    pub mimetype: &'static str,
}

impl SuggestRemoteSettingsClient for MockRemoteSettingsClient {
    fn get_records(
        &self,
        request: SuggestRemoteSettingsRecordRequest,
    ) -> Result<Vec<SuggestRemoteSettingsRecord>> {
        let record_type = request.record_type.unwrap_or_else(|| {
            panic!("MockRemoteSettingsClient.get_records: record_type required ")
        });
        // Note: limit and modified time are ignored
        Ok(match self.records.get(&record_type) {
            Some(records) => records.clone(),
            None => vec![],
        })
    }
}
