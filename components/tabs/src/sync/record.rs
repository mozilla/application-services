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
                let icon: Option<String> = tabs_obj
                    .get("icon")
                    .map(|i| i.as_str().unwrap_or_default().to_string());
                let last_used = parse_last_used(&tabs_obj["lastUsed"]);

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

fn parse_last_used(last_used_val: &JsonValue) -> u64 {
    // In order to support older clients, we are handling `last used` values that
    // are either floats, integers, or stringified floats or integers. We attempt to
    // represent `last used` as a float before converting it to an integer. If that
    // operation isn't successful, we try converting `last used` to an integer directly.
    // If that isn't successful, the returned value will be zero.

    if last_used_val.is_string() {
        let l = last_used_val.as_str().unwrap_or_default();

        match l.parse::<f64>() {
            Ok(f) => f.trunc() as u64,
            Err(_) => l.parse::<u64>().unwrap_or_default(),
        }
    } else {
        match last_used_val.as_f64() {
            Some(f) => f.trunc() as u64,
            None => last_used_val.as_u64().unwrap_or_default(),
        }
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
            },
            {
                "title": "Example2",
                "urlHistory": [
                    "example2.com"
                ],
                "icon": "example2.png",
                "lastUsed": "1623745144" // test with stringified integer value
            },
            {
                "title": "Test2",
                "urlHistory": [
                    "test2.com"
                ],
                "icon": "test2.png",
                "lastUsed": "1623745018.99" // test with stringified float value
            },
        ]);

        data.insert("tabs".to_string(), tabs);
        data.insert("clientName".to_string(), JsonValue::from("Nightly"));
        data.insert("ttl".to_string(), JsonValue::from(0));

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
                TabsRecordTab {
                    title: "Example2".to_string(),
                    url_history: vec!["example2.com".to_string()],
                    icon: Some("example2.png".to_string()),
                    last_used: 1623745144,
                },
                TabsRecordTab {
                    title: "Test2".to_string(),
                    url_history: vec!["test2.com".to_string()],
                    icon: Some("test2.png".to_string()),
                    last_used: 1623745018,
                },
            ],
            ttl: 0,
        };

        assert_eq!(actual, expected);

        Ok(())
    }
}
