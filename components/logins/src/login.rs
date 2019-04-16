/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::util;
use rusqlite::Row;
use serde_derive::*;
use std::time::{self, SystemTime};
use sync15::ServerTimestamp;

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    // TODO: consider `#[serde(rename = "id")] pub guid: String` to avoid confusion
    pub id: String,

    pub hostname: String,

    // rename_all = "camelCase" by default will do formSubmitUrl, but we can just
    // override this one field.
    #[serde(rename = "formSubmitURL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_submit_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_realm: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username: String,

    pub password: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username_field: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub password_field: String,

    #[serde(default)]
    pub time_created: i64,

    #[serde(default)]
    pub time_password_changed: i64,

    #[serde(default)]
    pub time_last_used: i64,

    #[serde(default)]
    pub times_used: i64,
}

fn string_or_default(row: &Row<'_>, col: &str) -> Result<String> {
    Ok(row.get::<_, Option<String>>(col)?.unwrap_or_default())
}

impl Login {
    #[inline]
    pub fn guid(&self) -> &String {
        &self.id
    }

    #[inline]
    pub fn guid_str(&self) -> &str {
        self.id.as_str()
    }

    pub fn check_valid(&self) -> Result<()> {
        if self.hostname.is_empty() {
            throw!(InvalidLogin::EmptyHostname);
        }

        if self.password.is_empty() {
            throw!(InvalidLogin::EmptyPassword);
        }

        if self.form_submit_url.is_some() && self.http_realm.is_some() {
            throw!(InvalidLogin::BothTargets);
        }

        if self.form_submit_url.is_none() && self.http_realm.is_none() {
            throw!(InvalidLogin::NoTarget);
        }
        Ok(())
    }

    pub(crate) fn from_row(row: &Row<'_>) -> Result<Login> {
        Ok(Login {
            id: row.get("guid")?,
            password: row.get("password")?,
            username: string_or_default(row, "username")?,

            hostname: row.get("hostname")?,
            http_realm: row.get("httpRealm")?,

            form_submit_url: row.get("formSubmitURL")?,

            username_field: string_or_default(row, "usernameField")?,
            password_field: string_or_default(row, "passwordField")?,

            time_created: row.get("timeCreated")?,
            // Might be null
            time_last_used: row
                .get::<_, Option<i64>>("timeLastUsed")?
                .unwrap_or_default(),

            time_password_changed: row.get("timePasswordChanged")?,
            times_used: row.get("timesUsed")?,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MirrorLogin {
    pub login: Login,
    pub is_overridden: bool,
    pub server_modified: ServerTimestamp,
}

impl MirrorLogin {
    #[inline]
    pub fn guid_str(&self) -> &str {
        self.login.guid_str()
    }

    pub(crate) fn from_row(row: &Row<'_>) -> Result<MirrorLogin> {
        Ok(MirrorLogin {
            login: Login::from_row(row)?,
            is_overridden: row.get("is_overridden")?,
            server_modified: ServerTimestamp(row.get::<_, i64>("server_modified")? as f64 / 1000.0),
        })
    }
}

// This doesn't really belong here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub(crate) enum SyncStatus {
    Synced = 0,
    Changed = 1,
    New = 2,
}

impl SyncStatus {
    #[inline]
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(SyncStatus::Synced),
            1 => Ok(SyncStatus::Changed),
            2 => Ok(SyncStatus::New),
            v => throw!(ErrorKind::BadSyncStatus(v)),
        }
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
    server_modified: ServerTimestamp(0.0)
});

// Stores data needed to do a 3-way merge
pub(crate) struct SyncLoginData {
    pub guid: String,
    pub local: Option<LocalLogin>,
    pub mirror: Option<MirrorLogin>,
    // None means it's a deletion
    pub inbound: (Option<Login>, ServerTimestamp),
}

impl SyncLoginData {
    #[inline]
    pub fn guid_str(&self) -> &str {
        &self.guid[..]
    }

    #[inline]
    pub fn guid(&self) -> &String {
        &self.guid
    }

    #[inline]
    pub fn from_payload(payload: sync15::Payload, ts: ServerTimestamp) -> Result<Self> {
        let guid = payload.id.clone();
        let login: Option<Login> = if payload.is_tombstone() {
            None
        } else {
            let record: Login = payload.into_record()?;
            Some(record)
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
    pub hostname: Option<String>,
    pub password: Option<String>,
    pub username: Option<String>,
    pub http_realm: Option<String>,
    pub form_submit_url: Option<String>,

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
    #[allow(clippy::cyclomatic_complexity)] // Looks like clippy considers this after macro-expansion...
    pub fn merge(self, mut b: LoginDelta, b_is_newer: bool) -> LoginDelta {
        let mut merged = self;
        merge_field!(merged, b, b_is_newer, hostname);
        merge_field!(merged, b, b_is_newer, password);
        merge_field!(merged, b, b_is_newer, username);
        merge_field!(merged, b, b_is_newer, http_realm);
        merge_field!(merged, b, b_is_newer, form_submit_url);

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
        apply_field!(self, delta, hostname);

        apply_field!(self, delta, password);
        apply_field!(self, delta, username);

        apply_field!(self, delta, time_created);
        apply_field!(self, delta, time_last_used);
        apply_field!(self, delta, time_password_changed);

        apply_field!(self, delta, password_field);
        apply_field!(self, delta, username_field);

        // Use Some("") to indicate that it should be changed to be None (hacky...)
        if let Some(realm) = delta.http_realm.take() {
            self.http_realm = if realm.is_empty() { None } else { Some(realm) };
        }

        if let Some(url) = delta.form_submit_url.take() {
            self.form_submit_url = if url.is_empty() { None } else { Some(url) };
        }

        self.times_used += delta.times_used;
    }

    pub(crate) fn delta(&self, older: &Login) -> LoginDelta {
        let mut delta = LoginDelta::default();

        if self.form_submit_url != older.form_submit_url {
            delta.form_submit_url = Some(self.form_submit_url.clone().unwrap_or_default());
        }

        if self.http_realm != older.http_realm {
            delta.http_realm = Some(self.http_realm.clone().unwrap_or_default());
        }

        if self.hostname != older.hostname {
            delta.hostname = Some(self.hostname.clone());
        }
        if self.username != older.username {
            delta.username = Some(self.username.clone());
        }
        if self.password != older.password {
            delta.password = Some(self.password.clone());
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
