/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use serde::de::Error;
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
        // Note: We are hand-parsing the tabs payload so that we can provide support for
        // older clients that may have a stringified float or integer `last_used` value.
        let id = payload.id.to_string();
        let client_name =
            parse_string_from_json(&"clientName", payload.data.get("clientName"), false)?
                .expect("client name to have a value");
        let tabs: Vec<TabsRecordTab> = payload
            .data
            .get("tabs")
            .ok_or_else(|| serde_json::Error::custom("missing field `tabs`"))?
            .as_array()
            .ok_or_else(|| serde_json::Error::custom("invalid `tabs`, expected sequence"))?
            .iter()
            .map(|x| -> Result<TabsRecordTab> {
                let tabs_obj: Map<String, JsonValue> = x.as_object().unwrap_or(&Map::new()).clone();
                let title: String = parse_string_from_json(&"title", tabs_obj.get("title"), false)?
                    .expect("tab title to have a value");
                let url_history: Vec<String> = tabs_obj
                    .get("urlHistory")
                    .ok_or_else(|| serde_json::Error::custom("missing field `urlHistory`"))?
                    .as_array()
                    .ok_or_else(|| {
                        serde_json::Error::custom("invalid `urlHistory`, expected sequence")
                    })?
                    .iter()
                    .map(|u| -> Result<String> {
                        Ok(u.as_str()
                            .ok_or_else(|| {
                                serde_json::Error::custom(
                                    "invalid `urlHistory` value, expected string",
                                )
                            })?
                            .to_string())
                    })
                    .into_iter()
                    .collect::<Result<_>>()?;
                let icon = parse_string_from_json("icon", tabs_obj.get("icon"), true)?;
                let last_used = parse_last_used(
                    tabs_obj
                        .get("lastUsed")
                        .ok_or_else(|| serde_json::Error::custom("missing field `lastUsed`"))?,
                )?;

                Ok(TabsRecordTab {
                    title,
                    url_history,
                    icon,
                    last_used,
                })
            })
            .into_iter()
            .collect::<Result<_>>()?;

        let ttl: u32 = match payload.data.get("ttl") {
            Some(v) => v.as_u64().unwrap_or_default() as u32,
            None => u32::default(),
        };

        Ok(TabsRecord {
            id,
            client_name,
            tabs,
            ttl,
        })
    }
}

fn parse_last_used(last_used_val: &JsonValue) -> Result<u64> {
    // In order to support older clients, we are handling `last used` values that
    // are either floats, integers, or stringified floats or integers. We attempt to
    // represent `last used` as a float before converting it to an integer. If that
    // operation isn't successful, we try converting `last used` to an integer directly.
    // If that isn't successful, we return an error.

    let invalid_err_msg = "invalid `lastUsed`, expected u64";
    let last_used = if last_used_val.is_string() {
        let l = last_used_val.as_str().unwrap_or_default();

        match l.parse::<f64>() {
            Ok(f) => f.trunc() as u64,
            Err(_) => l
                .parse::<u64>()
                .map_err(|_| serde_json::Error::custom(invalid_err_msg))?,
        }
    } else {
        match last_used_val.as_f64() {
            Some(f) => f.trunc() as u64,
            None => last_used_val
                .as_u64()
                .ok_or_else(|| serde_json::Error::custom(invalid_err_msg))?,
        }
    };

    Ok(last_used)
}

fn parse_string_from_json(
    field_name: &str,
    val: Option<&JsonValue>,
    can_be_empty: bool,
) -> Result<Option<String>> {
    if can_be_empty && val.is_none() {
        Ok(None)
    } else {
        Ok(Some(
            val.ok_or_else(|| {
                serde_json::Error::custom(format!("missing field `{}`", field_name))
            })?
            .as_str()
            .ok_or_else(|| {
                serde_json::Error::custom(format!("invalid `{}`, expected string", field_name))
            })?
            .to_string(),
        ))
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

        let data = json!({
            "clientName": "Nightly",
            "tabs": [
                {
                    "title": "Example",
                    "urlHistory": [
                        "example.com",
                        "example2.com"
                    ],
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
            ],
            "ttl": 0
        })
        .as_object()
        .unwrap()
        .clone();

        let actual = TabsRecord::from_payload(sync15::Payload {
            id: guid.clone(),
            deleted: false,
            data,
        })?;

        let expected = TabsRecord {
            id: guid.to_string(),
            client_name: "Nightly".to_string(),
            tabs: vec![
                TabsRecordTab {
                    title: "Example".to_string(),
                    url_history: vec!["example.com".to_string(), "example2.com".to_string()],
                    icon: None,
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

    #[test]
    fn test_tabs_record_from_payload_with_invalid_data() -> Result<()> {
        struct TestCase {
            case_description: String,
            data: JsonValue,
            expected_err_msg: String,
        }

        let test_cases = vec![
            TestCase {
                case_description: "test missing client name".to_string(),
                data: json!({
                    "tabs": []
                }),
                expected_err_msg: "missing field `clientName`".to_string(),
            },
            TestCase {
                case_description: "test invalid client name".to_string(),
                data: json!({
                    "clientName": 0,
                }),
                expected_err_msg: "invalid `clientName`, expected string".to_string(),
            },
            TestCase {
                case_description: "test missing tabs".to_string(),
                data: json!({
                    "clientName": "",
                }),
                expected_err_msg: "missing field `tabs`".to_string(),
            },
            TestCase {
                case_description: "test invalid tabs".to_string(),
                data: json!({
                    "clientName": "",
                    "tabs": 0,
                }),
                expected_err_msg: "invalid `tabs`, expected sequence".to_string(),
            },
            TestCase {
                case_description: "test missing title".to_string(),
                data: json!({
                    "clientName": "Nightly",
                    "tabs": [
                        {
                            "urlHistory": []
                        }
                    ],
                    "ttl": 0
                }),
                expected_err_msg: "missing field `title`".to_string(),
            },
            TestCase {
                case_description: "test invalid title".to_string(),
                data: json!({
                    "clientName": "Nightly",
                    "tabs": [
                        {
                            "title": false
                        }
                    ],
                    "ttl": 0
                }),
                expected_err_msg: "invalid `title`, expected string".to_string(),
            },
            TestCase {
                case_description: "test missing url history".to_string(),
                data: json!({
                    "clientName": "Nightly",
                    "tabs": [
                        {
                            "title": ""
                        }
                    ],
                    "ttl": 0
                }),
                expected_err_msg: "missing field `urlHistory`".to_string(),
            },
            TestCase {
                case_description: "test invalid url history".to_string(),
                data: json!({
                    "clientName": "Nightly",
                    "tabs": [
                        {
                            "title": "",
                            "urlHistory": 0
                        }
                    ],
                    "ttl": 0
                }),
                expected_err_msg: "invalid `urlHistory`, expected sequence".to_string(),
            },
            TestCase {
                case_description: "test invalid url history values".to_string(),
                data: json!({
                    "clientName": "Nightly",
                    "tabs": [
                        {
                            "title": "",
                            "urlHistory": [0, 2, 3]
                        }
                    ],
                    "ttl": 0
                }),
                expected_err_msg: "invalid `urlHistory` value, expected string".to_string(),
            },
            TestCase {
                case_description: "test invalid icon".to_string(),
                data: json!({
                        "clientName": "Nightly",
                        "tabs": [
                            {
                                "title": "",
                                "urlHistory": [],
                                "icon": [],
                                "lastUsed": 0
                            }
                        ],
                "ttl": 0
                    }),
                expected_err_msg: "invalid `icon`, expected string".to_string(),
            },
            TestCase {
                case_description: "test missing last used".to_string(),
                data: json!({
                    "clientName": "Nightly",
                    "tabs": [
                        {
                            "title": "",
                            "urlHistory": [],
                            "icon": ""
                        }
                    ],
                    "ttl": 0
                }),
                expected_err_msg: "missing field `lastUsed`".to_string(),
            },
            TestCase {
                case_description: "test invalid last used".to_string(),
                data: json!({
                    "clientName": "Nightly",
                    "tabs": [
                        {
                            "title": "",
                            "urlHistory": [],
                            "lastUsed": true
                        }
                    ],
                    "ttl": 0
                }),
                expected_err_msg: "invalid `lastUsed`, expected u64".to_string(),
            },
        ];

        for tc in test_cases {
            let actual = TabsRecord::from_payload(sync15::Payload {
                id: Guid::random(),
                deleted: false,
                data: tc.data.as_object().unwrap().clone(),
            });

            assert!(actual.is_err(), "{}", tc.case_description);
            assert_eq!(
                actual.err().unwrap().to_string(),
                format!("Error parsing JSON data: {}", tc.expected_err_msg)
            );
        }

        Ok(())
    }
}
