/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::super::telemetry;
/// The Send Tab functionality is backed by Firefox Accounts device commands.
/// A device shows it can handle "Send Tab" commands by advertising the "open-uri"
/// command in its on own device record.
/// This command data bundle contains a one-time generated `PublicCommandKeys`
/// (while keeping locally `PrivateCommandKeys` containing the private key),
/// wrapped by the account oldsync scope `kSync` to form a `CommandKeysPayload`.
///
/// When a device sends a tab to another, it decrypts that `CommandKeysPayload` using `kSync`,
/// uses the obtained public key to encrypt the `SendTabPayload` it created that
/// contains the tab to send and finally forms the encrypted payload that is
/// then sent to the target device.
use serde_derive::*;

pub const COMMAND_NAME: &str = "https://identity.mozilla.com/cmd/open-uri";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendTabPayload {
    pub entries: Vec<TabHistoryEntry>,
    #[serde(rename = "flowID", default)]
    pub flow_id: String,
    #[serde(rename = "streamID", default)]
    pub stream_id: String,
}

impl From<SendTabPayload> for crate::SendTabPayload {
    fn from(payload: SendTabPayload) -> Self {
        crate::SendTabPayload {
            entries: payload.entries.into_iter().map(From::from).collect(),
            flow_id: payload.flow_id,
            stream_id: payload.stream_id,
        }
    }
}

impl SendTabPayload {
    pub fn single_tab(title: &str, url: &str) -> (Self, telemetry::SentCommand) {
        let sent_telemetry: telemetry::SentCommand = telemetry::SentCommand::for_send_tab();
        (
            SendTabPayload {
                entries: vec![TabHistoryEntry {
                    title: title.to_string(),
                    url: url.to_string(),
                }],
                flow_id: sent_telemetry.flow_id.clone(),
                stream_id: sent_telemetry.stream_id.clone(),
            },
            sent_telemetry,
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabHistoryEntry {
    pub title: String,
    pub url: String,
}

impl From<TabHistoryEntry> for crate::TabHistoryEntry {
    fn from(e: TabHistoryEntry) -> Self {
        crate::TabHistoryEntry {
            title: e.title,
            url: e.url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_parse_payload() {
        let minimal = r#"{ "entries": []}"#;
        let payload: SendTabPayload = serde_json::from_str(minimal).expect("should work");
        assert_eq!(payload.flow_id, "".to_string());
    }

    #[test]
    fn test_payload() {
        let (payload, telem) = SendTabPayload::single_tab("title", "http://example.com");
        let json = serde_json::to_string(&payload).expect("should work");
        assert_eq!(telem.flow_id.len(), 12);
        assert_eq!(telem.stream_id.len(), 12);
        assert_ne!(telem.flow_id, telem.stream_id);
        let p2: SendTabPayload = serde_json::from_str(&json).expect("should work");
        // no 'PartialEq' derived so check each field individually...
        assert_eq!(payload.entries[0].url, "http://example.com".to_string());
        assert_eq!(payload.flow_id, p2.flow_id);
        assert_eq!(payload.stream_id, p2.stream_id);
    }
}
