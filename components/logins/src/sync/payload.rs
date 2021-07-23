/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Login entry from a server payload
//
// This struct is used for fetching/sending login records to the server.  There are a number
// of differences between this and the top-level Login struct; some fields are renamed, some are
// locally encrypted, etc.
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::Login;
use serde_derive::*;
use sync_guid::Guid;

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoginPayload {
    #[serde(rename = "id")]
    pub guid: Guid,

    pub origin: String,

    // rename_all = "camelCase" by default will do formActionOrigin, but we can just
    // override this one field.
    #[serde(rename = "formActionOrigin")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_action_origin: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_realm: Option<String>,

    #[serde(default)]
    pub username: String,

    pub password: String,

    #[serde(default)]
    pub username_field: String,

    #[serde(default)]
    pub password_field: String,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub time_created: i64,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub time_password_changed: i64,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub time_last_used: i64,

    #[serde(default)]
    pub times_used: i64,
}

// These probably should be on the payload itself, but one refactor at a time!
impl Login {
    pub fn from_payload(
        sync_payload: sync15::Payload,
        encdec: &EncryptorDecryptor,
    ) -> Result<Self> {
        let p: crate::sync::LoginPayload = sync_payload.into_record()?;

        Ok(Login {
            id: p.guid.to_string(),
            origin: p.origin,
            form_action_origin: p.form_action_origin,
            http_realm: p.http_realm,
            username_enc: encdec.encrypt(&p.username)?,
            password_enc: encdec.encrypt(&p.password)?,
            username_field: p.username_field,
            password_field: p.password_field,
            time_created: p.time_created,
            time_password_changed: p.time_password_changed,
            time_last_used: p.time_last_used,
            times_used: p.times_used,
        })
    }

    pub fn into_payload(self, encdec: &EncryptorDecryptor) -> Result<sync15::Payload> {
        Ok(sync15::Payload::from_record(crate::sync::LoginPayload {
            guid: self.guid(),
            origin: self.origin,
            form_action_origin: self.form_action_origin,
            http_realm: self.http_realm,
            username: encdec.decrypt(&self.username_enc)?,
            password: encdec.decrypt(&self.password_enc)?,
            username_field: self.username_field,
            password_field: self.password_field,
            time_created: self.time_created,
            time_password_changed: self.time_password_changed,
            time_last_used: self.time_last_used,
            times_used: self.times_used,
        })?)
    }
}

// Quiet clippy, since this function is passed to deserialiaze_with...
#[allow(clippy::unnecessary_wraps)]
fn deserialize_timestamp<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    use serde::de::Deserialize;
    // Invalid and negative timestamps are all replaced with 0. Eventually we
    // should investigate replacing values that are unreasonable but still fit
    // in an i64 (a date 1000 years in the future, for example), but
    // appropriately handling that is complex.
    Ok(i64::deserialize(deserializer).unwrap_or_default().max(0))
}

#[cfg(test)]
mod tests {
    use crate::encryption::test_utils::{decrypt, encrypt, TEST_ENCRYPTOR};
    use crate::sync::merge::SyncLoginData;
    use crate::Login;
    use sync15::ServerTimestamp;

    #[test]
    fn test_payload_to_login() {
        let payload: sync15::Payload = serde_json::from_value(serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "origin": "https://www.example.com",
            "username": "user",
            "password": "password",
        }))
        .unwrap();
        let login = Login::from_payload(payload, &TEST_ENCRYPTOR).unwrap();
        assert_eq!(login.id, "123412341234");
        assert_eq!(login.http_realm, Some("test".to_string()));
        assert_eq!(login.origin, "https://www.example.com");
        assert_eq!(decrypt(&login.username_enc), "user");
        assert_eq!(decrypt(&login.password_enc), "password");
    }

    #[test]
    fn test_login_into_payload() {
        let login = Login {
            id: "123412341234".into(),
            http_realm: Some("test".into()),
            origin: "https://www.example.com".into(),
            username_enc: encrypt("user"),
            password_enc: encrypt("password"),
            ..Login::default()
        };
        let payload = login.into_payload(&TEST_ENCRYPTOR).unwrap();

        assert_eq!(payload.id, "123412341234");
        assert_eq!(payload.deleted, false);
        assert_eq!(payload.data["httpRealm"], "test".to_string());
        assert_eq!(payload.data["origin"], "https://www.example.com");
        assert_eq!(payload.data["username"], "user");
        assert_eq!(payload.data["password"], "password");
        assert!(!payload.data.contains_key("formActionOrigin"));
    }

    #[test]
    fn test_username_field_requires_a_form_target() {
        let bad_payload: sync15::Payload = serde_json::from_value(serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "origin": "https://www.example.com",
            "username": "test",
            "password": "test",
            "usernameField": "invalid"
        }))
        .unwrap();

        let login = Login::from_payload(bad_payload.clone(), &TEST_ENCRYPTOR).unwrap();
        assert_eq!(login.username_field, "invalid");
        assert!(login.check_valid().is_err());
        assert_eq!(login.fixup().unwrap().username_field, "");

        // Incoming sync data gets fixed automatically.
        let login =
            SyncLoginData::from_payload(bad_payload, ServerTimestamp::default(), &TEST_ENCRYPTOR)
                .unwrap()
                .inbound
                .0
                .unwrap();
        assert_eq!(login.username_field, "");
    }

    #[test]
    fn test_password_field_requires_a_form_target() {
        let bad_payload: sync15::Payload = serde_json::from_value(serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "origin": "https://www.example.com",
            "username": "test",
            "password": "test",
            "passwordField": "invalid"
        }))
        .unwrap();

        let login = Login::from_payload(bad_payload, &TEST_ENCRYPTOR).unwrap();
        assert_eq!(login.password_field, "invalid");
        assert!(login.check_valid().is_err());
        assert_eq!(login.fixup().unwrap().password_field, "");
    }
}
