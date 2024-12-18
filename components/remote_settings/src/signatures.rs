
use core::clone::Clone;

use crate::{
    RemoteSettingsRecord,
    Result,
};
use canonical_json;
use serde_json::{json, Value};


fn select_record_fields(value: &Value) -> Value {
    if let Value::Object(map) = value {
        let new_map = map
            .iter()
            .filter_map(|(key, v)| {
                if key == "deleted" || key == "attachment" && v.is_null() {
                    None
                } else {
                    Some((key.clone(), v.clone()))
                }
            })
            .collect();
        Value::Object(new_map)
    } else {
        value.clone() // Return the value as-is if it's not an object
    }
}

/// Serialize collection data into canonical JSON. This must match the server implementation.
fn serialize_data(timestamp: u64, records: &[RemoteSettingsRecord]) -> Result<Vec<u8>> {
    let mut sorted_records = records.to_vec();
    sorted_records.sort_by_cached_key(|r| r.id.clone());
    let serialized = canonical_json::to_string(&json!({
        "data": sorted_records.into_iter().map(|r| select_record_fields(&json!(r))).collect::<Vec<Value>>(),
        "last_modified": timestamp.to_string()
    }))?;
    let data = format!("Content-Signature:\x00{}", serialized);
    Ok(data.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use crate::{RemoteSettingsRecord, Attachment};
    use serde_json::json;
    use super::serialize_data;

    #[test]
    fn test_records_canonicaljson_serialization() {
        let bytes = serialize_data(
            1337,
            &vec![RemoteSettingsRecord {
                last_modified: 42,
                id: "bonjour".into(),
                deleted: false,
                attachment: None,
                fields: json!({"foo": "bar"}).as_object().unwrap().clone(),
            }],
        )
        .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert_eq!(s, "Content-Signature:\u{0}{\"data\":[{\"id\":\"bonjour\",\"last_modified\":42,\"foo\":\"bar\"}],\"last_modified\":\"1337\"}");
    }

    #[test]
    fn test_records_canonicaljson_serialization_with_attachment() {
        let bytes = serialize_data(
            1337,
            &vec![RemoteSettingsRecord {
                last_modified: 42,
                id: "bonjour".into(),
                deleted: true,
                attachment: Some(Attachment {
                    filename: "pix.jpg".into(),
                    mimetype: "image/jpeg".into(),
                    location: "folder/file.jpg".into(),
                    hash: "aabbcc".into(),
                    size: 1234567,
                }),
                fields: json!({}).as_object().unwrap().clone(),
            }],
        )
        .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert_eq!(s, "Content-Signature:\0{\"data\":[{\"id\":\"bonjour\",\"last_modified\":42,\"attachment\":{\"filename\":\"pix.jpg\",\"mimetype\":\"image/jpeg\",\"location\":\"folder/file.jpg\",\"hash\":\"aabbcc\",\"size\":1234567}}],\"last_modified\":\"1337\"}");
    }
}
