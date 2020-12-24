/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod incoming;

use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecordData {
    #[serde(default)]
    pub given_name: String,

    #[serde(default)]
    pub additional_name: String,

    #[serde(default)]
    pub family_name: String,

    #[serde(default)]
    pub organization: String,

    #[serde(default)]
    pub street_address: String,

    #[serde(default)]
    pub address_level3: String,

    #[serde(default)]
    pub address_level2: String,

    #[serde(default)]
    pub address_level1: String,

    #[serde(default)]
    pub postal_code: String,

    #[serde(default)]
    pub country: String,

    #[serde(default)]
    pub tel: String,

    #[serde(default)]
    pub email: String,

    #[serde(default)]
    pub time_created: Option<Timestamp>,

    #[serde(default)]
    pub time_last_used: Option<Timestamp>,

    #[serde(default)]
    pub time_last_modified: Option<Timestamp>,

    #[serde(default)]
    pub times_used: Option<i64>,

    #[serde(default)]
    pub sync_change_counter: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    #[serde(rename = "id", default)]
    pub guid: SyncGuid,

    #[serde(flatten)]
    data: RecordData,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddressChanges {
    #[serde(rename = "id")]
    pub guid: SyncGuid,

    pub old_value: RecordData,

    pub new_value: Record,
}
