/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::Row;

use crate::error::Result;
use crate::internal::crypto::KeyV1 as Key;

use super::types::Timestamp;

pub type ChannelID = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushRecord {
    /// Designation label provided by the subscribing service
    pub channel_id: ChannelID,

    /// Endpoint provided from the push server
    pub endpoint: String,

    /// The receipient (service worker)'s scope
    pub scope: String,

    /// Private EC Prime256v1 key info.
    pub key: Vec<u8>,

    /// Time this subscription was created.
    pub ctime: Timestamp,

    /// VAPID public key to restrict subscription updates for only those that sign
    /// using the private VAPID key.
    pub app_server_key: Option<String>,
}

impl PushRecord {
    /// Create a Push Record from the Subscription info: endpoint, encryption
    /// keys, etc.
    pub fn new(chid: &str, endpoint: &str, scope: &str, key: Key) -> Self {
        // XXX: unwrap
        Self {
            channel_id: chid.to_owned(),
            endpoint: endpoint.to_owned(),
            scope: scope.to_owned(),
            key: key.serialize().unwrap(),
            ctime: Timestamp::now(),
            app_server_key: None,
        }
    }

    pub(crate) fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(PushRecord {
            channel_id: row.get("channel_id")?,
            endpoint: row.get("endpoint")?,
            scope: row.get("scope")?,
            key: row.get("key")?,
            ctime: row.get("ctime")?,
            app_server_key: row.get("app_server_key")?,
        })
    }
}
