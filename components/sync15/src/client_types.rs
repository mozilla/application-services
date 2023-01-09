/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module has to be here because of some hard-to-avoid hacks done for the
//! tabs engine... See issue #2590

use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;

/// Argument to Store::prepare_for_sync. See comment there for more info. Only
/// really intended to be used by tabs engine.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientData {
    pub local_client_id: String,
    /// A hashmap of records in the `clients` collection. Key is the id of the record in
    /// that collection, which may or may not be the device's fxa_device_id.
    pub recent_clients: HashMap<String, RemoteClient>,
}

/// Information about a remote client in the clients collection.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RemoteClient {
    pub fxa_device_id: Option<String>,
    pub device_name: String,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_option_device_type")]
    #[serde(serialize_with = "serialize_option_device_type")]
    pub device_type: Option<DeviceType>,
}

fn deserialize_option_device_type<'de, D>(d: D) -> std::result::Result<Option<DeviceType>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let opt_device_type: Option<DeviceType> = Option::deserialize(d)?;
    Ok(match opt_device_type {
        Some(val) => match val {
            DeviceType::Unknown => None,
            _ => Some(val),
        },
        None => None,
    })
}

fn serialize_option_device_type<S>(id: &Option<DeviceType>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match id {
        Some(DeviceType::Unknown) => s.serialize_none(),
        Some(d) => d.serialize(s),
        None => s.serialize_none(),
    }
}

/// Enumeration for the different types of device.
///
/// Firefox Accounts and the broader Sync universe separates devices into broad categories for
/// display purposes, such as distinguishing a desktop PC from a mobile phone.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceType {
    #[serde(rename = "desktop")]
    Desktop,
    #[serde(rename = "mobile")]
    Mobile,
    #[serde(rename = "tablet")]
    Tablet,
    #[serde(rename = "vr")]
    VR,
    #[serde(rename = "tv")]
    TV,
    // Unknown is a bit odd - it should never be set (ie, it's never serialized)
    // and exists really just so we can avoid using an Option<>.
    #[serde(other)]
    #[serde(skip_serializing)] // Don't you dare trying.
    Unknown,
}

#[cfg(test)]
mod device_type_tests {
    use super::*;

    #[test]
    fn test_serde_ser() {
        assert_eq!(
            serde_json::to_string(&DeviceType::Desktop).unwrap(),
            "\"desktop\""
        );
        assert_eq!(
            serde_json::to_string(&DeviceType::Mobile).unwrap(),
            "\"mobile\""
        );
        assert_eq!(
            serde_json::to_string(&DeviceType::Tablet).unwrap(),
            "\"tablet\""
        );
        assert_eq!(serde_json::to_string(&DeviceType::VR).unwrap(), "\"vr\"");
        assert_eq!(serde_json::to_string(&DeviceType::TV).unwrap(), "\"tv\"");
        assert!(serde_json::to_string(&DeviceType::Unknown).is_err());
    }

    #[test]
    fn test_serde_de() {
        assert!(matches!(
            serde_json::from_str::<DeviceType>("\"desktop\"").unwrap(),
            DeviceType::Desktop
        ));
        assert!(matches!(
            serde_json::from_str::<DeviceType>("\"mobile\"").unwrap(),
            DeviceType::Mobile
        ));
        assert!(matches!(
            serde_json::from_str::<DeviceType>("\"tablet\"").unwrap(),
            DeviceType::Tablet
        ));
        assert!(matches!(
            serde_json::from_str::<DeviceType>("\"vr\"").unwrap(),
            DeviceType::VR
        ));
        assert!(matches!(
            serde_json::from_str::<DeviceType>("\"tv\"").unwrap(),
            DeviceType::TV
        ));
        assert!(matches!(
            serde_json::from_str::<DeviceType>("\"something-else\"").unwrap(),
            DeviceType::Unknown,
        ));
    }

    #[test]
    fn test_remote_client() {
        // Missing `device_type` gets None.
        let dt = serde_json::from_str::<RemoteClient>("{\"device_name\": \"foo\"}").unwrap();
        assert_eq!(dt.device_type, None);
        // But reserializes as null.
        assert_eq!(
            serde_json::to_string(&dt).unwrap(),
            "{\"fxa_device_id\":null,\"device_name\":\"foo\",\"device_type\":null}"
        );

        // Unknown device_type string deserializes as None.
        let dt = serde_json::from_str::<RemoteClient>(
            "{\"device_name\": \"foo\", \"device_type\": \"foo\"}",
        )
        .unwrap();
        assert_eq!(dt.device_type, None);
        // The None gets re-serialized as null.
        assert_eq!(
            serde_json::to_string(&dt).unwrap(),
            "{\"fxa_device_id\":null,\"device_name\":\"foo\",\"device_type\":null}"
        );

        // Some(DeviceType::Unknown) gets serialized as null.
        let dt = RemoteClient {
            device_name: "bar".to_string(),
            fxa_device_id: None,
            device_type: Some(DeviceType::Unknown),
        };
        assert_eq!(
            serde_json::to_string(&dt).unwrap(),
            "{\"fxa_device_id\":null,\"device_name\":\"bar\",\"device_type\":null}"
        );

        // Some(DeviceType::Desktop) gets serialized as "desktop".
        let dt = RemoteClient {
            device_name: "bar".to_string(),
            fxa_device_id: Some("fxa".to_string()),
            device_type: Some(DeviceType::Desktop),
        };
        assert_eq!(
            serde_json::to_string(&dt).unwrap(),
            "{\"fxa_device_id\":\"fxa\",\"device_name\":\"bar\",\"device_type\":\"desktop\"}"
        );
    }

    #[test]
    fn test_client_data() {
        let client_data = ClientData {
            local_client_id: "my-device".to_string(),
            recent_clients: HashMap::from([
                (
                    "my-device".to_string(),
                    RemoteClient {
                        fxa_device_id: None,
                        device_name: "my device".to_string(),
                        device_type: None,
                    },
                ),
                (
                    "device-no-tabs".to_string(),
                    RemoteClient {
                        fxa_device_id: None,
                        device_name: "device with no tabs".to_string(),
                        device_type: None,
                    },
                ),
                (
                    "device-with-a-tab".to_string(),
                    RemoteClient {
                        fxa_device_id: None,
                        device_name: "device with a tab".to_string(),
                        device_type: Some(DeviceType::Desktop),
                    },
                ),
            ]),
        };
        //serialize
        let client_data_ser = serde_json::to_string(&client_data).unwrap();
        // deserialize
        let client_data_des: ClientData = serde_json::from_str(&client_data_ser).unwrap();
        assert_eq!(client_data_des, client_data);
    }
}
