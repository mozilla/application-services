/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{history_sync::ServerVisitTimestamp, types::UnknownFields};
use serde_derive::*;
use sync_guid::Guid as SyncGuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecordVisit {
    pub date: ServerVisitTimestamp,
    #[serde(rename = "type")]
    pub transition: u8,

    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    // TODO: consider `#[serde(rename = "id")] pub guid: String` to avoid confusion
    pub id: SyncGuid,

    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,

    pub hist_uri: String,

    pub visits: Vec<HistoryRecordVisit>,

    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}
