/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Merging for Sync.
use super::SyncStatus;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::login::EncryptedLogin;
use crate::util;
use rusqlite::Row;
use std::time::{self, SystemTime};
use sync15::ServerTimestamp;
use sync_guid::Guid;

#[derive(Clone, Debug)]
pub(crate) struct MirrorLogin {
    pub login: EncryptedLogin,
    pub is_overridden: bool,
    pub server_modified: ServerTimestamp,
}

impl MirrorLogin {
    #[inline]
    pub fn guid_str(&self) -> &str {
        &self.login.record.id
    }

    pub(crate) fn from_row(row: &Row<'_>) -> Result<MirrorLogin> {
        Ok(MirrorLogin {
            login: EncryptedLogin::from_row(row)?,
            is_overridden: row.get("is_overridden")?,
            server_modified: ServerTimestamp(row.get::<_, i64>("server_modified")?),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LocalLogin {
    pub login: EncryptedLogin,
    pub sync_status: SyncStatus,
    pub is_deleted: bool,
    pub local_modified: SystemTime,
}

impl LocalLogin {
    #[inline]
    pub fn guid_str(&self) -> &str {
        self.login.guid_str()
    }

    pub(crate) fn from_row(row: &Row<'_>) -> Result<LocalLogin> {
        Ok(LocalLogin {
            login: EncryptedLogin::from_row(row)?,
            sync_status: SyncStatus::from_u8(row.get("sync_status")?)?,
            is_deleted: row.get("is_deleted")?,
            local_modified: util::system_time_millis_from_row(row, "local_modified")?,
        })
    }
}

macro_rules! impl_login {
    ($ty:ty { $($fields:tt)* }) => {
        impl AsRef<EncryptedLogin> for $ty {
            #[inline]
            fn as_ref(&self) -> &EncryptedLogin {
                &self.login
            }
        }

        impl AsMut<EncryptedLogin> for $ty {
            #[inline]
            fn as_mut(&mut self) -> &mut EncryptedLogin {
                &mut self.login
            }
        }

        impl From<$ty> for EncryptedLogin {
            #[inline]
            fn from(l: $ty) -> Self {
                l.login
            }
        }

        impl From<EncryptedLogin> for $ty {
            #[inline]
            fn from(login: EncryptedLogin) -> Self {
                Self { login, $($fields)* }
            }
        }
    };
}

impl_login!(LocalLogin {
    sync_status: SyncStatus::New,
    is_deleted: false,
    local_modified: time::UNIX_EPOCH
});

impl_login!(MirrorLogin {
    is_overridden: false,
    server_modified: ServerTimestamp(0)
});

// Stores data needed to do a 3-way merge
#[derive(Debug)]
pub(crate) struct SyncLoginData {
    pub guid: Guid,
    pub local: Option<LocalLogin>,
    pub mirror: Option<MirrorLogin>,
    // None means it's a deletion
    pub inbound: (Option<EncryptedLogin>, ServerTimestamp),
}

impl SyncLoginData {
    #[inline]
    pub fn guid_str(&self) -> &str {
        self.guid.as_str()
    }

    #[inline]
    pub fn guid(&self) -> &Guid {
        &self.guid
    }

    pub fn from_payload(
        payload: sync15::Payload,
        ts: ServerTimestamp,
        encdec: &EncryptorDecryptor,
    ) -> Result<Self> {
        let guid = payload.id.clone();
        let login: Option<EncryptedLogin> = if payload.is_tombstone() {
            None
        } else {
            // from_payload does fixups if necessary and possible.
            Some(EncryptedLogin::from_payload(payload, encdec)?)
        };
        Ok(Self {
            guid,
            local: None,
            mirror: None,
            inbound: (login, ts),
        })
    }
}

macro_rules! impl_login_setter {
    ($setter_name:ident, $field:ident, $Login:ty) => {
        impl SyncLoginData {
            pub(crate) fn $setter_name(&mut self, record: $Login) -> Result<()> {
                // TODO: We probably shouldn't panic in this function!
                if self.$field.is_some() {
                    // Shouldn't be possible (only could happen if UNIQUE fails in sqlite, or if we
                    // get duplicate guids somewhere,but we check).
                    panic!(
                        "SyncLoginData::{} called on object that already has {} data",
                        stringify!($setter_name),
                        stringify!($field)
                    );
                }

                if self.guid_str() != record.guid_str() {
                    // This is almost certainly a bug in our code.
                    panic!(
                        "Wrong guid on login in {}: {:?} != {:?}",
                        stringify!($setter_name),
                        self.guid_str(),
                        record.guid_str()
                    );
                }

                self.$field = Some(record);
                Ok(())
            }
        }
    };
}

impl_login_setter!(set_local, local, LocalLogin);
impl_login_setter!(set_mirror, mirror, MirrorLogin);

#[derive(Debug, Default, Clone)]
pub(crate) struct LoginDelta {
    // "non-commutative" fields
    pub origin: Option<String>,
    pub password: Option<String>,
    pub username: Option<String>,
    pub http_realm: Option<String>,
    pub form_action_origin: Option<String>,

    pub time_created: Option<i64>,
    pub time_last_used: Option<i64>,
    pub time_password_changed: Option<i64>,

    // "non-conflicting" fields (which are the same)
    pub password_field: Option<String>,
    pub username_field: Option<String>,

    // Commutative field
    pub times_used: i64,
}

macro_rules! merge_field {
    ($merged:ident, $b:ident, $prefer_b:expr, $field:ident) => {
        if let Some($field) = $b.$field.take() {
            if $merged.$field.is_some() {
                log::warn!("Collision merging login field {}", stringify!($field));
                if $prefer_b {
                    $merged.$field = Some($field);
                }
            } else {
                $merged.$field = Some($field);
            }
        }
    };
}

impl LoginDelta {
    #[allow(clippy::cognitive_complexity)] // Looks like clippy considers this after macro-expansion...
    pub fn merge(self, mut b: LoginDelta, b_is_newer: bool) -> LoginDelta {
        let mut merged = self;
        merge_field!(merged, b, b_is_newer, origin);
        merge_field!(merged, b, b_is_newer, password);
        merge_field!(merged, b, b_is_newer, username);
        merge_field!(merged, b, b_is_newer, http_realm);
        merge_field!(merged, b, b_is_newer, form_action_origin);

        merge_field!(merged, b, b_is_newer, time_created);
        merge_field!(merged, b, b_is_newer, time_last_used);
        merge_field!(merged, b, b_is_newer, time_password_changed);

        merge_field!(merged, b, b_is_newer, password_field);
        merge_field!(merged, b, b_is_newer, username_field);

        // commutative fields
        merged.times_used += b.times_used;

        merged
    }
}

macro_rules! apply_field {
    ($login:ident, $delta:ident, $field:ident) => {
        if let Some($field) = $delta.$field.take() {
            $login.fields.$field = $field.into();
        }
    };
}

macro_rules! apply_metadata_field {
    ($login:ident, $delta:ident, $field:ident) => {
        if let Some($field) = $delta.$field.take() {
            $login.record.$field = $field.into();
        }
    };
}

impl EncryptedLogin {
    pub(crate) fn apply_delta(
        &mut self,
        mut delta: LoginDelta,
        encdec: &EncryptorDecryptor,
    ) -> Result<()> {
        apply_field!(self, delta, origin);

        apply_metadata_field!(self, delta, time_created);
        apply_metadata_field!(self, delta, time_last_used);
        apply_metadata_field!(self, delta, time_password_changed);

        apply_field!(self, delta, password_field);
        apply_field!(self, delta, username_field);

        let mut sec_fields = self.decrypt_fields(encdec)?;
        if let Some(password) = delta.password.take() {
            sec_fields.password = password;
        }
        if let Some(username) = delta.username.take() {
            sec_fields.username = username;
        }
        self.sec_fields = encdec.encrypt_struct(&sec_fields)?;

        // Use Some("") to indicate that it should be changed to be None (hacky...)
        if let Some(realm) = delta.http_realm.take() {
            self.fields.http_realm = if realm.is_empty() { None } else { Some(realm) };
        }

        if let Some(url) = delta.form_action_origin.take() {
            self.fields.form_action_origin = if url.is_empty() { None } else { Some(url) };
        }

        self.record.times_used += delta.times_used;
        Ok(())
    }

    pub(crate) fn delta(
        &self,
        older: &EncryptedLogin,
        encdec: &EncryptorDecryptor,
    ) -> Result<LoginDelta> {
        let mut delta = LoginDelta::default();

        if self.fields.form_action_origin != older.fields.form_action_origin {
            delta.form_action_origin =
                Some(self.fields.form_action_origin.clone().unwrap_or_default());
        }

        if self.fields.http_realm != older.fields.http_realm {
            delta.http_realm = Some(self.fields.http_realm.clone().unwrap_or_default());
        }

        if self.fields.origin != older.fields.origin {
            delta.origin = Some(self.fields.origin.clone());
        }
        let older_sec_fields = older.decrypt_fields(encdec)?;
        let self_sec_fields = self.decrypt_fields(encdec)?;
        if self_sec_fields.username != older_sec_fields.username {
            delta.username = Some(self_sec_fields.username.clone());
        }
        if self_sec_fields.password != older_sec_fields.password {
            delta.password = Some(self_sec_fields.password);
        }
        if self.fields.password_field != older.fields.password_field {
            delta.password_field = Some(self.fields.password_field.clone());
        }
        if self.fields.username_field != older.fields.username_field {
            delta.username_field = Some(self.fields.username_field.clone());
        }

        // We discard zero (and negative numbers) for timestamps so that a
        // record that doesn't contain this information (these are
        // `#[serde(default)]`) doesn't skew our records.
        //
        // Arguably, we should also also ignore values later than our
        // `time_created`, or earlier than our `time_last_used` or
        // `time_password_changed`. Doing this properly would probably require
        // a scheme analogous to Desktop's weak-reupload system, so I'm punting
        // on it for now.
        if self.record.time_created > 0 && self.record.time_created != older.record.time_created {
            delta.time_created = Some(self.record.time_created);
        }
        if self.record.time_last_used > 0
            && self.record.time_last_used != older.record.time_last_used
        {
            delta.time_last_used = Some(self.record.time_last_used);
        }
        if self.record.time_password_changed > 0
            && self.record.time_password_changed != older.record.time_password_changed
        {
            delta.time_password_changed = Some(self.record.time_password_changed);
        }

        if self.record.times_used > 0 && self.record.times_used != older.record.times_used {
            delta.times_used = self.record.times_used - older.record.times_used;
        }

        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::test_utils::TEST_ENCRYPTOR;

    #[test]
    fn test_invalid_payload_timestamps() {
        #[allow(clippy::unreadable_literal)]
        let bad_timestamp = 18446732429235952000u64;
        let bad_payload: sync15::Payload = serde_json::from_value(serde_json::json!({
            "id": "123412341234",
            "formSubmitURL": "https://www.example.com/submit",
            "hostname": "https://www.example.com",
            "username": "test",
            "password": "test",
            "timeCreated": bad_timestamp,
            "timeLastUsed": "some other garbage",
            "timePasswordChanged": -30, // valid i64 but negative
        }))
        .unwrap();
        let login =
            SyncLoginData::from_payload(bad_payload, ServerTimestamp::default(), &TEST_ENCRYPTOR)
                .unwrap()
                .inbound
                .0
                .unwrap();
        assert_eq!(login.record.time_created, 0);
        assert_eq!(login.record.time_last_used, 0);
        assert_eq!(login.record.time_password_changed, 0);

        let now64 = util::system_time_ms_i64(std::time::SystemTime::now());
        let good_payload: sync15::Payload = serde_json::from_value(serde_json::json!({
            "id": "123412341234",
            "formSubmitURL": "https://www.example.com/submit",
            "hostname": "https://www.example.com",
            "username": "test",
            "password": "test",
            "timeCreated": now64 - 100,
            "timeLastUsed": now64 - 50,
            "timePasswordChanged": now64 - 25,
        }))
        .unwrap();

        let login =
            SyncLoginData::from_payload(good_payload, ServerTimestamp::default(), &TEST_ENCRYPTOR)
                .unwrap()
                .inbound
                .0
                .unwrap();

        assert_eq!(login.record.time_created, now64 - 100);
        assert_eq!(login.record.time_last_used, now64 - 50);
        assert_eq!(login.record.time_password_changed, now64 - 25);
    }
}
