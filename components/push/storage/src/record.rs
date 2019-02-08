use rusqlite::Row;

use crate::types::Timestamp;
use crypto::Key;
use push_errors::{Error, Result};

pub type ChannelID = String;

#[derive(Clone, Debug, PartialEq)]
pub struct PushRecord {
    // Designation label provided by the subscribing service
    pub channel_id: ChannelID,

    // Endpoint provided from the push server
    pub endpoint: String,

    // XXX:
    pub scope: String,

    // An originAttributes to suffix string (XXX: related to scope)
    pub origin_attributes: String,

    // Private EC Prime256v1 key info. (Public key can be derived from this)
    pub key: Vec<u8>,

    // Is this as priviledged system record
    pub system_record: bool,

    // List of the most recent message IDs from the server.
    pub recent_message_ids: Vec<String>,

    // Number of pushes for this record
    pub push_count: u8,

    // Last push rec'vd
    pub last_push: Timestamp,

    // Time this subscription was created.
    pub ctime: Timestamp,

    // Max quota count for sub
    pub quota: u8,

    // VAPID public key to restrict subscription updates for only those that sign
    // using the private VAPID key.
    pub app_server_key: Option<String>,

    // (if this is a bridged connection (e.g. on Android), this is the native OS Push ID)
    pub native_id: Option<String>,
}

impl PushRecord {
    /// Create a Push Record from the Subscription info: endpoint, encryption
    /// keys, etc.
    pub fn new(
        _uaid: &str,
        chid: &str,
        endpoint: &str,
        scope: &str,
        origin_attributes: &str,
        private_key: Key,
        system_record: bool,
    ) -> Self {
        // XXX: unwrap
        Self {
            channel_id: chid.to_owned(),
            endpoint: endpoint.to_owned(),
            scope: scope.to_owned(),
            origin_attributes: origin_attributes.to_owned(),
            key: private_key.serialize().unwrap(),
            system_record: system_record,
            recent_message_ids: vec![],
            push_count: 0,
            last_push: 0.into(), // XXX: consider null instead
            ctime: Timestamp::now(),
            quota: 0,
            app_server_key: None,
            native_id: None,
        }
    }

    pub(crate) fn from_row(row: &Row) -> Result<Self> {
        Ok(PushRecord {
            channel_id: row.get_checked("channel_id")?,
            endpoint: row.get_checked("endpoint")?,
            scope: row.get_checked("scope")?,
            origin_attributes: row.get_checked("origin_attributes")?,
            key: row.get_checked("key")?,
            system_record: row.get_checked("system_record")?,
            recent_message_ids: serde_json::from_str(
                &row.get_checked::<_, String>("recent_message_ids")?,
            )
            .map_err(|e| Error::internal(&format!("Deserializing recent_message_ids: {}", e)))?,
            push_count: row.get_checked("push_count")?,
            last_push: row.get_checked("last_push")?,
            ctime: row.get_checked("ctime")?,
            quota: row.get_checked("quota")?,
            app_server_key: row.get_checked("app_server_key")?,
            native_id: row.get_checked("native_id")?,
        })
    }

    pub(crate) fn increment(&mut self) -> Result<Self> {
        self.push_count += 1;
        self.last_push = Timestamp::now();
        // TODO: check for quotas, etc
        Ok(self.clone())
    }
}
