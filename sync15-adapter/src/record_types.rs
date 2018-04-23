/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use key_bundle::KeyBundle;
use errors::{ErrorKind, Result};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordRecord {
    id: String,
    hostname: Option<String>,

    #[serde(rename = "formSubmitURL")]
    form_submit_url: Option<String>,

    #[serde(rename = "httpRealm")]
    http_realm: Option<String>,

    username: String,
    password: String,

    #[serde(rename = "usernameField")]
    #[serde(default = "")]
    username_field: String,

    #[serde(rename = "passwordField")]
    #[serde(default = "")]
    password_field: String,

    #[serde(rename = "timeCreated")]
    time_created: i64,

    #[serde(rename = "timePasswordChanged")]
    time_password_changed: i64,

}


