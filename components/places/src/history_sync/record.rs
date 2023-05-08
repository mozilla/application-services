/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{history_sync::ServerVisitTimestamp, types::UnknownFields};
use serde::Deserialize;
use serde_derive::*;
use sync_guid::Guid as SyncGuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecordVisit {
    pub date: ServerVisitTimestamp,
    #[serde(rename = "type")]
    pub transition: u8,

    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    // TODO: consider `#[serde(rename = "id")] pub guid: String` to avoid confusion
    pub id: SyncGuid,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_nonull_string")]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,

    pub hist_uri: String,

    pub visits: Vec<HistoryRecordVisit>,

    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

fn deserialize_nonull_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match <Option<String>>::deserialize(deserializer)? {
        Some(s) => s,
        None => "".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_title() {
        // #5544 tells us we are seeing an explicit null for an incoming tab title.
        // Really not clear where these are coming from - possibly very old versions of
        // apps, but seems easy to handle, so here we are!
        let json = serde_json::json!({
            "id": "foo",
            "title": null,
            "histUri": "https://example.com",
            "visits": [],
        });

        let rec = serde_json::from_value::<HistoryRecord>(json).expect("should deser");
        assert!(rec.title.is_empty());
    }

    #[test]
    fn test_missing_title() {
        let json = serde_json::json!({
            "id": "foo",
            "histUri": "https://example.com",
            "visits": [],
        });

        let rec = serde_json::from_value::<HistoryRecord>(json).expect("should deser");
        assert!(rec.title.is_empty());
    }
}
