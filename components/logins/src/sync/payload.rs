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
use crate::login::ValidateAndFixup;
use crate::SecureLoginFields;
use crate::{EncryptedLogin, LoginFields, RecordFields};
use serde_derive::*;
use sync15::bso::OutgoingBso;
use sync_guid::Guid;

type UnknownFields = serde_json::Map<String, serde_json::Value>;

/// What we get from the server after parsing the payload. We need to round-trip "unknown"
/// fields, but don't want to carry them around in `EncryptedLogin`.
#[derive(Debug)]
pub(super) struct IncomingLogin {
    pub login: EncryptedLogin,
    // An encrypted UnknownFields, or None if there are none.
    pub unknown: Option<String>,
}

impl IncomingLogin {
    pub fn guid(&self) -> Guid {
        self.login.guid()
    }

    pub(super) fn from_incoming_payload(
        p: LoginPayload,
        encdec: &EncryptorDecryptor,
    ) -> Result<Self> {
        let fields = LoginFields {
            origin: p.hostname,
            form_action_origin: p.form_submit_url,
            http_realm: p.http_realm,
            username_field: p.username_field,
            password_field: p.password_field,
        };
        let sec_fields = SecureLoginFields {
            username: p.username,
            password: p.password,
        };
        // We handle NULL in the DB for migrated databases and it's wasteful
        // to encrypt the common case of an empty map, so...
        let unknown = if p.unknown_fields.is_empty() {
            None
        } else {
            Some(encdec.encrypt_struct(&p.unknown_fields)?)
        };

        // If we can't fix the parts we keep the invalid bits.
        Ok(Self {
            login: EncryptedLogin {
                record: RecordFields {
                    id: p.guid.into(),
                    time_created: p.time_created,
                    time_password_changed: p.time_password_changed,
                    time_last_used: p.time_last_used,
                    times_used: p.times_used,
                },
                fields: fields.maybe_fixup()?.unwrap_or(fields),
                sec_fields: sec_fields
                    .maybe_fixup()?
                    .unwrap_or(sec_fields)
                    .encrypt(encdec)?,
            },
            unknown,
        })
    }
}

/// The JSON payload that lives on the storage servers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoginPayload {
    #[serde(rename = "id")]
    pub guid: Guid,

    // This is 'origin' in our Login struct.
    pub hostname: String,

    // This is 'form_action_origin' in our Login struct.
    // rename_all = "camelCase" by default will do formSubmitUrl, but we can just
    // override this one field.
    #[serde(rename = "formSubmitURL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_submit_url: Option<String>,

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

    // Additional "unknown" round-tripped fields.
    #[serde(flatten)]
    unknown_fields: UnknownFields,
}

// These probably should be on the payload itself, but one refactor at a time!
impl EncryptedLogin {
    pub fn into_bso(
        self,
        encdec: &EncryptorDecryptor,
        enc_unknown_fields: Option<String>,
    ) -> Result<OutgoingBso> {
        let unknown_fields = match enc_unknown_fields {
            Some(s) => encdec.decrypt_struct::<UnknownFields>(&s)?,
            None => Default::default(),
        };
        let sec_fields: SecureLoginFields = encdec.decrypt_struct(&self.sec_fields)?;
        Ok(OutgoingBso::from_content_with_id(
            crate::sync::LoginPayload {
                guid: self.guid(),
                hostname: self.fields.origin,
                form_submit_url: self.fields.form_action_origin,
                http_realm: self.fields.http_realm,
                username_field: self.fields.username_field,
                password_field: self.fields.password_field,
                username: sec_fields.username,
                password: sec_fields.password,
                time_created: self.record.time_created,
                time_password_changed: self.record.time_password_changed,
                time_last_used: self.record.time_last_used,
                times_used: self.record.times_used,
                unknown_fields,
            },
        )?)
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
    use super::*;
    use crate::encryption::test_utils::{encrypt_struct, TEST_ENCRYPTOR};
    use crate::sync::merge::SyncLoginData;
    use crate::{EncryptedLogin, LoginFields, RecordFields, SecureLoginFields};
    use sync15::bso::IncomingBso;

    #[test]
    fn test_payload_to_login() {
        let bso = IncomingBso::from_test_content(serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "hostname": "https://www.example.com",
            "username": "user",
            "password": "password",
        }));
        let login = IncomingLogin::from_incoming_payload(
            bso.into_content::<LoginPayload>().content().unwrap(),
            &TEST_ENCRYPTOR,
        )
        .unwrap()
        .login;
        assert_eq!(login.record.id, "123412341234");
        assert_eq!(login.fields.http_realm, Some("test".to_string()));
        assert_eq!(login.fields.origin, "https://www.example.com");
        assert_eq!(login.fields.form_action_origin, None);
        let sec_fields = login.decrypt_fields(&TEST_ENCRYPTOR).unwrap();
        assert_eq!(sec_fields.username, "user");
        assert_eq!(sec_fields.password, "password");
    }

    #[test]
    fn test_payload_unknown_fields() {
        // No "unknown" fields.
        let bso = IncomingBso::from_test_content(serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "hostname": "https://www.example.com",
            "username": "user",
            "password": "password",
        }));
        let payload = bso.into_content::<LoginPayload>().content().unwrap();
        assert!(payload.unknown_fields.is_empty());

        // An unknown "foo"
        let bso = IncomingBso::from_test_content(serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "hostname": "https://www.example.com",
            "username": "user",
            "password": "password",
            "foo": "bar",
        }));
        let payload = bso.into_content::<LoginPayload>().content().unwrap();
        assert_eq!(payload.unknown_fields.len(), 1);
        assert_eq!(
            payload.unknown_fields.get("foo").unwrap().as_str().unwrap(),
            "bar"
        );
        // re-serialize it.
        let unknown = Some(
            TEST_ENCRYPTOR
                .encrypt_struct::<UnknownFields>(&payload.unknown_fields)
                .unwrap(),
        );
        let login = IncomingLogin::from_incoming_payload(payload, &TEST_ENCRYPTOR)
            .unwrap()
            .login;
        // The raw outgoing payload should have it back.
        let outgoing = login.into_bso(&TEST_ENCRYPTOR, unknown).unwrap();
        let json =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&outgoing.payload)
                .unwrap();
        assert_eq!(json.get("foo").unwrap().as_str().unwrap(), "bar");
    }

    #[test]
    fn test_form_submit_payload_to_login() {
        let bso = IncomingBso::from_test_content(serde_json::json!({
            "id": "123412341234",
            "hostname": "https://www.example.com",
            "formSubmitURL": "https://www.example.com",
            "usernameField": "username-field",
            "username": "user",
            "password": "password",
        }));
        let login = IncomingLogin::from_incoming_payload(
            bso.into_content::<LoginPayload>().content().unwrap(),
            &TEST_ENCRYPTOR,
        )
        .unwrap()
        .login;
        assert_eq!(login.record.id, "123412341234");
        assert_eq!(login.fields.http_realm, None);
        assert_eq!(login.fields.origin, "https://www.example.com");
        assert_eq!(
            login.fields.form_action_origin,
            Some("https://www.example.com".to_string())
        );
        assert_eq!(login.fields.username_field, "username-field");
        let sec_fields = login.decrypt_fields(&TEST_ENCRYPTOR).unwrap();
        assert_eq!(sec_fields.username, "user");
        assert_eq!(sec_fields.password, "password");
    }

    #[test]
    fn test_login_into_payload() {
        let login = EncryptedLogin {
            record: RecordFields {
                id: "123412341234".into(),
                ..Default::default()
            },
            fields: LoginFields {
                http_realm: Some("test".into()),
                origin: "https://www.example.com".into(),
                ..Default::default()
            },
            sec_fields: encrypt_struct(&SecureLoginFields {
                username: "user".into(),
                password: "password".into(),
            }),
        };
        let bso = login.into_bso(&TEST_ENCRYPTOR, None).unwrap();
        assert_eq!(bso.envelope.id, "123412341234");
        let payload_data: serde_json::Value = serde_json::from_str(&bso.payload).unwrap();
        assert_eq!(payload_data["httpRealm"], "test".to_string());
        assert_eq!(payload_data["hostname"], "https://www.example.com");
        assert_eq!(payload_data["username"], "user");
        assert_eq!(payload_data["password"], "password");
        assert!(matches!(
            payload_data["formActionOrigin"],
            serde_json::Value::Null
        ));
    }

    #[test]
    fn test_username_field_requires_a_form_target() {
        let bad_json = serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "hostname": "https://www.example.com",
            "username": "test",
            "password": "test",
            "usernameField": "invalid"
        });
        let bad_bso = IncomingBso::from_test_content(bad_json.clone());

        // Incoming sync data gets fixed automatically.
        let login = IncomingLogin::from_incoming_payload(
            bad_bso.into_content::<LoginPayload>().content().unwrap(),
            &TEST_ENCRYPTOR,
        )
        .unwrap()
        .login;
        assert_eq!(login.fields.username_field, "");

        // SyncLoginData::from_payload also fixes up.
        let bad_bso = IncomingBso::from_test_content(bad_json);
        let login = SyncLoginData::from_bso(bad_bso, &TEST_ENCRYPTOR)
            .unwrap()
            .inbound
            .unwrap()
            .login;
        assert_eq!(login.fields.username_field, "");
    }

    #[test]
    fn test_password_field_requires_a_form_target() {
        let bad_bso = IncomingBso::from_test_content(serde_json::json!({
            "id": "123412341234",
            "httpRealm": "test",
            "hostname": "https://www.example.com",
            "username": "test",
            "password": "test",
            "passwordField": "invalid"
        }));

        let login = IncomingLogin::from_incoming_payload(
            bad_bso.into_content::<LoginPayload>().content().unwrap(),
            &TEST_ENCRYPTOR,
        )
        .unwrap()
        .login;
        assert_eq!(login.fields.password_field, "");
    }
}
