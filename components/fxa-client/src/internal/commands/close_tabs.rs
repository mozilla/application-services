/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::internal::telemetry;
use serde_derive::*;

pub const COMMAND_NAME: &str = "https://identity.mozilla.com/cmd/close-uri/v1";
// Note: matches REMOTE_COMMAND_TTL_MS in tabs storage.rs
pub const COMMAND_TTL: u64 = 2 * 24 * 3600;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloseTabsPayload {
    pub urls: Vec<String>,
    #[serde(rename = "flowID", default)]
    pub flow_id: String,
    #[serde(rename = "streamID", default)]
    pub stream_id: String,
}

impl From<CloseTabsPayload> for crate::CloseTabsPayload {
    fn from(payload: CloseTabsPayload) -> Self {
        crate::CloseTabsPayload { urls: payload.urls }
    }
}

impl CloseTabsPayload {
    pub fn with_urls(urls: Vec<String>) -> (Self, telemetry::SentCommand) {
        let sent_telemetry: telemetry::SentCommand = telemetry::SentCommand::for_close_tabs();
        (Self::with_telemetry(&sent_telemetry, urls), sent_telemetry)
    }

    pub fn with_telemetry(sent_telemetry: &telemetry::SentCommand, urls: Vec<String>) -> Self {
        CloseTabsPayload {
            urls,
            flow_id: sent_telemetry.flow_id.clone(),
            stream_id: sent_telemetry.stream_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Result;

    #[test]
    fn test_empty_payload() -> Result<()> {
        let empty = r#"{ "urls": []}"#;
        let payload: CloseTabsPayload = serde_json::from_str(empty)?;
        assert!(payload.urls.is_empty());

        Ok(())
    }

    #[test]
    fn test_payload() -> Result<()> {
        let (payload, telem) = CloseTabsPayload::with_urls(vec!["https://www.mozilla.org".into()]);
        let json = serde_json::to_string(&payload)?;
        assert!(!json.is_empty());
        assert_eq!(telem.flow_id.len(), 12);
        assert_eq!(telem.stream_id.len(), 12);
        assert_ne!(telem.flow_id, telem.stream_id);

        let deserialized: CloseTabsPayload = serde_json::from_str(&json)?;
        assert_eq!(deserialized, payload);

        Ok(())
    }
}
