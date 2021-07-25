/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Merging for Sync.
use super::SyncStatus;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::util;
use crate::Login;
use rusqlite::Row;
use std::time::{self, SystemTime};
use sync15::ServerTimestamp;
use sync_guid::Guid;

#[derive(Clone, Debug)]
pub(crate) struct MirrorLogin {
    pub login: Login,
    pub is_overridden: bool,
    pub server_modified: ServerTimestamp,
}

impl MirrorLogin {
    #[inline]
    pub fn guid_str(&self) -> &str {
        &self.login.id
    }

    pub(crate) fn from_row(row: &Row<'_>) -> Result<MirrorLogin> {
        Ok(MirrorLogin {
            login: Login::from_row(row)?,
            is_overridden: row.get("is_overridden")?,
            server_modified: ServerTimestamp(row.get::<_, i64>("server_modified")?),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LocalLogin {
    pub login: Login,
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
            login: Login::from_row(row)?,
            sync_status: SyncStatus::from_u8(row.get("sync_status")?)?,
            is_deleted: row.get("is_deleted")?,
            local_modified: util::system_time_millis_from_row(row, "local_modified")?,
        })
    }
}

macro_rules! impl_login {
    ($ty:ty { $($fields:tt)* }) => {
        impl AsRef<Login> for $ty {
            #[inline]
            fn as_ref(&self) -> &Login {
                &self.login
            }
        }

        impl AsMut<Login> for $ty {
            #[inline]
            fn as_mut(&mut self) -> &mut Login {
                &mut self.login
            }
        }

        impl From<$ty> for Login {
            #[inline]
            fn from(l: $ty) -> Self {
                l.login
            }
        }

        impl From<Login> for $ty {
            #[inline]
            fn from(login: Login) -> Self {
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
    pub inbound: (Option<Login>, ServerTimestamp),
}

impl SyncLoginData {
    #[inline]
    pub fn guid_str(&self) -> &str {
        &self.guid.as_str()
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
        let login: Option<Login> = if payload.is_tombstone() {
            None
        } else {
            let record = Login::from_payload(payload, encdec)?;
            // If we can fixup incoming records from sync, do so.
            // But if we can't then keep the invalid data.
            record.maybe_fixup().unwrap_or(None).or(Some(record))
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
    pub password_enc: Option<String>,
    pub username_enc: Option<String>,
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
        merge_field!(merged, b, b_is_newer, password_enc);
        merge_field!(merged, b, b_is_newer, username_enc);
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
            $login.$field = $field.into();
        }
    };
}

impl Login {
    pub(crate) fn apply_delta(&mut self, mut delta: LoginDelta) {
        apply_field!(self, delta, origin);

        apply_field!(self, delta, password_enc);
        apply_field!(self, delta, username_enc);

        apply_field!(self, delta, time_created);
        apply_field!(self, delta, time_last_used);
        apply_field!(self, delta, time_password_changed);

        apply_field!(self, delta, password_field);
        apply_field!(self, delta, username_field);

        // Use Some("") to indicate that it should be changed to be None (hacky...)
        if let Some(realm) = delta.http_realm.take() {
            self.http_realm = if realm.is_empty() { None } else { Some(realm) };
        }

        if let Some(url) = delta.form_action_origin.take() {
            self.form_action_origin = if url.is_empty() { None } else { Some(url) };
        }

        self.times_used += delta.times_used;
    }

    pub(crate) fn delta(&self, older: &Login) -> LoginDelta {
        let mut delta = LoginDelta::default();

        if self.form_action_origin != older.form_action_origin {
            delta.form_action_origin = Some(self.form_action_origin.clone().unwrap_or_default());
        }

        if self.http_realm != older.http_realm {
            delta.http_realm = Some(self.http_realm.clone().unwrap_or_default());
        }

        if self.origin != older.origin {
            delta.origin = Some(self.origin.clone());
        }
        // TODO-sqlcipher -- should we be decrypting these?
        if self.username_enc != older.username_enc {
            delta.username_enc = Some(self.username_enc.clone());
        }
        if self.password_enc != older.password_enc {
            delta.password_enc = Some(self.password_enc.clone());
        }
        if self.password_field != older.password_field {
            delta.password_field = Some(self.password_field.clone());
        }
        if self.username_field != older.username_field {
            delta.username_field = Some(self.username_field.clone());
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
        if self.time_created > 0 && self.time_created != older.time_created {
            delta.time_created = Some(self.time_created);
        }
        if self.time_last_used > 0 && self.time_last_used != older.time_last_used {
            delta.time_last_used = Some(self.time_last_used);
        }
        if self.time_password_changed > 0
            && self.time_password_changed != older.time_password_changed
        {
            delta.time_password_changed = Some(self.time_password_changed);
        }

        if self.times_used > 0 && self.times_used != older.times_used {
            delta.times_used = self.times_used - older.times_used;
        }

        delta
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
        assert_eq!(login.time_created, 0);
        assert_eq!(login.time_last_used, 0);
        assert_eq!(login.time_password_changed, 0);

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

        assert_eq!(login.time_created, now64 - 100);
        assert_eq!(login.time_last_used, now64 - 50);
        assert_eq!(login.time_password_changed, now64 - 25);
    }
}
