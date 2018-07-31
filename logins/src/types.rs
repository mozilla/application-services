// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

///! This module defines some core types that support Sync 1.5 passwords and arbitrary logins.

use std::convert::{
    AsRef,
};

use mentat::{
    DateTime,
    Utc,
    Uuid,
};

/// Firefox Sync password records must have at least a formSubmitURL or httpRealm, but not both.
#[derive(PartialEq, Eq, Hash, Clone, Debug)] // , Serialize, Deserialize)]
pub enum FormTarget {
    // #[serde(rename = "httpRealm")]
    HttpRealm(String),

    // #[serde(rename = "formSubmitURL")]
    FormSubmitURL(String),
}

#[derive(PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct SyncGuid(pub String);

impl AsRef<str> for SyncGuid {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<T> From<T> for SyncGuid where T: Into<String> {
    fn from(x: T) -> SyncGuid {
        SyncGuid(x.into())
    }
}

/// A Sync 1.5 password record.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct ServerPassword {
    /// The UUID of this record, returned by the remote server as part of this record's envelope.
    ///
    /// For historical reasons, Sync 1.5 passwords use a UUID rather than a (9 character) GUID like
    /// other collections.
    pub uuid: SyncGuid,

    /// The time last modified, returned by the remote server as part of this record's envelope.
    pub modified: DateTime<Utc>,

    /// Material fields.  A password without a username corresponds to an XXX.
    pub hostname: String,
    pub username: Option<String>,
    pub password: String,

    pub target: FormTarget,

    /// Metadata.  Unfortunately, not all clients pass-through (let alone collect and propagate!)
    /// metadata correctly.
    pub times_used: usize,

    pub time_created: DateTime<Utc>,
    pub time_last_used: DateTime<Utc>,
    pub time_password_changed: DateTime<Utc>,

    /// Mostly deprecated: these fields were once used to help with form fill.
    pub username_field: Option<String>,
    pub password_field: Option<String>,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct CredentialId(pub String);

impl AsRef<str> for CredentialId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl CredentialId {
    pub fn random() -> Self {
        CredentialId(Uuid::new_v4().hyphenated().to_string())
    }
}

impl<T> From<T> for CredentialId where T: Into<String> {
    fn from(x: T) -> CredentialId {
        CredentialId(x.into())
    }
}

/// A username/password pair, optionally decorated with a user-specified title.
///
/// A credential is uniquely identified by its `id`.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Credential {
    /// A stable opaque identifier uniquely naming this credential.
    pub id: CredentialId,

    // The username associated to this credential.
    pub username: Option<String>,

    // The password associated to this credential.
    pub password: String,

    // When the credential was created.  This is best-effort: it's the timestamp observed by the
    // device on which the credential was created, which is incomparable with timestamps observed by
    // other devices in the constellation (including any servers).
    pub created_at: DateTime<Utc>,

    /// An optional user-specified title of this credential, like `My LDAP`.
    pub title: Option<String>,
}
