// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

///! This module implements special `serde` support for `ServerPassword` instances.
///!
///! Unfortunately, there doesn't seem to be a good way to directly deserialize `ServerPassword`
///! from JSON because of `target`. In theory `#[serde(flatten)]` on that property would do it, but
///! Firefox for Desktop writes records like `{"httpRealm": null, "formSubmitURL": "..."}`, e.g.,
///! where both fields are present, but one is `null`.  This breaks `serde`.  We therefore use a
///! custom serializer and deserializer through the `SerializablePassword` type.

use serde::{
    self,
    Deserializer,
    Serializer,
};

use mentat::{
    DateTime,
    FromMillis,
    ToMillis,
    Utc,
};

use types::{
    FormTarget,
    ServerPassword,
    SyncGuid,
};

fn zero_timestamp() -> DateTime<Utc> {
    DateTime::<Utc>::from_millis(0)
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SerializablePassword {
    pub id: String,
    pub hostname: String,

    #[serde(rename = "formSubmitURL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_submit_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_realm: Option<String>,

    #[serde(default)]
    pub username: Option<String>,
    pub password: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_field: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_field: Option<String>,

    #[serde(default)]
    pub time_created: i64,

    #[serde(default)]
    pub time_password_changed: i64,

    #[serde(default)]
    pub time_last_used: i64,

    #[serde(default)]
    pub times_used: usize,
}

impl From<ServerPassword> for SerializablePassword {
    fn from(sp: ServerPassword) -> SerializablePassword {
        let (form_submit_url, http_realm) = match sp.target {
            FormTarget::FormSubmitURL(url) => (Some(url), None),
            FormTarget::HttpRealm(realm)   => (None, Some(realm)),
        };
        SerializablePassword {
            id: sp.uuid.0,
            username_field: sp.username_field,
            password_field: sp.password_field,

            form_submit_url,
            http_realm,

            hostname: sp.hostname,
            username: sp.username,
            password: sp.password,

            times_used: sp.times_used,
            time_password_changed: sp.time_password_changed.to_millis(),
            time_last_used: sp.time_last_used.to_millis(),
            time_created: sp.time_created.to_millis(),
        }
    }
}

impl serde::ser::Serialize for ServerPassword {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SerializablePassword::from(self.clone()).serialize(serializer)
    }
}

impl<'de> serde::de::Deserialize<'de> for ServerPassword {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ServerPassword, D::Error> {
        let s = SerializablePassword::deserialize(deserializer)?;
        let target = match (s.form_submit_url, s.http_realm) {
            (Some(_), Some(_)) =>
                return Err(serde::de::Error::custom("ServerPassword has both formSubmitURL and httpRealm")),
            (None, None) =>
                return Err(serde::de::Error::custom("ServerPassword is missing both formSubmitURL and httpRealm")),
            (Some(url), None) =>
                FormTarget::FormSubmitURL(url),
            (None, Some(realm)) =>
                FormTarget::HttpRealm(realm),
        };

        Ok(ServerPassword {
            uuid: SyncGuid(s.id),
            modified: zero_timestamp(),
            hostname: s.hostname,
            username: s.username,
            password: s.password,
            target,
            username_field: s.username_field,
            password_field: s.password_field,
            times_used: s.times_used,
            time_created: FromMillis::from_millis(s.time_created),
            time_last_used: FromMillis::from_millis(s.time_last_used),
            time_password_changed: FromMillis::from_millis(s.time_password_changed),
        })
    }
}
