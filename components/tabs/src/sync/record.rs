/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use serde_derive::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TabsRecordTab {
    pub title: String,
    pub url_history: Vec<String>,
    pub icon: Option<String>,
    pub last_used: u64, // Seconds since epoch!
}

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabsRecord {
    pub id: String, // `String` instead of `SyncGuid` because some IDs are FxA device ID.
    pub client_name: String,
    pub tabs: Vec<TabsRecordTab>,
    #[serde(default)]
    pub ttl: u32,
}

impl TabsRecord {
    #[inline]
    pub fn from_payload(payload: sync15::Payload) -> Result<Self> {
        let client_name: String = payload.data["clientName"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let tabs: Vec<TabsRecordTab> = payload.data["tabs"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|x| {
                let tabs_obj: Map<String, JsonValue> = x.as_object().unwrap_or(&Map::new()).clone();
                let title: String = tabs_obj["title"].as_str().unwrap_or_default().to_string();
                let url_history: Vec<String> = tabs_obj["urlHistory"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .map(|u| u.as_str().unwrap_or_default().to_string())
                    .collect::<Vec<_>>();
                let icon: Option<String> = if let Some(i) = tabs_obj.get("icon") {
                    Some(i.as_str().unwrap_or_default().to_string())
                } else {
                    None
                };

                let last_used: u64 = if let Some(l) = tabs_obj.get("lastUsed") {
                    if l.is_f64() {
                        l.as_f64().unwrap_or_default().trunc() as u64
                    } else {
                        l.as_u64().unwrap_or_default()
                    }
                } else {
                    0
                };

                TabsRecordTab {
                    title,
                    url_history,
                    icon,
                    last_used,
                }
            })
            .collect::<Vec<_>>();

        let ttl: u32 = payload.data["ttl"].as_u64().unwrap_or_default() as u32;

        Ok(TabsRecord {
            id: payload.id.to_string(),
            client_name,
            tabs,
            ttl,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use sync_guid::Guid;

    #[test]
    fn test_tabs_record_from_payload() -> Result<()> {
        let guid = Guid::random();
        let mut data = Map::new();

        let tabs: JsonValue = json!([
            {
                "title": "Example",
                "urlHistory": [
                    "example.com",
                    "example2.com"
                ],
                "icon": "example.png",
                "lastUsed": 1623745123 // test with integer value
            },
            {
                "title": "Test",
                "urlHistory": [
                    "test.com",
                    "test2.com"
                ],
                "icon": "test.png",
                "lastUsed": 1623745000.99 // test with float value
            }
        ]);

        data.insert("clientName".to_string(), JsonValue::from("Nightly"));
        data.insert("tabs".to_string(), tabs);

        let payload_input = sync15::Payload {
            id: guid.clone(),
            deleted: false,
            data,
        };

        let actual = TabsRecord::from_payload(payload_input)?;
        let expected = TabsRecord {
            id: guid.to_string(),
            client_name: "Nightly".to_string(),
            tabs: vec![
                TabsRecordTab {
                    title: "Example".to_string(),
                    url_history: vec!["example.com".to_string(), "example2.com".to_string()],
                    icon: Some("example.png".to_string()),
                    last_used: 1623745123,
                },
                TabsRecordTab {
                    title: "Test".to_string(),
                    url_history: vec!["test.com".to_string(), "test2.com".to_string()],
                    icon: Some("test.png".to_string()),
                    last_used: 1623745000,
                },
            ],
            ttl: 0,
        };

        assert_eq!(actual, expected);

        Ok(())
    }
}
