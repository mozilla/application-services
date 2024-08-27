/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::internal::telemetry;
use serde_derive::*;

pub const COMMAND_NAME: &str = "https://identity.mozilla.com/cmd/close-inactive-tabs/v1";
pub const COMMAND_TTL: u64 = super::close_tabs::COMMAND_TTL;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloseAllInactiveTabsPayload {
    #[serde(rename = "flowID", default)]
    pub flow_id: String,
    #[serde(rename = "streamID", default)]
    pub stream_id: String,
}

impl CloseAllInactiveTabsPayload {
    pub fn new() -> (Self, telemetry::SentCommand) {
        let sent_telemetry: telemetry::SentCommand =
            telemetry::SentCommand::for_close_all_inactive_tabs();
        (
            CloseAllInactiveTabsPayload {
                flow_id: sent_telemetry.flow_id.clone(),
                stream_id: sent_telemetry.stream_id.clone(),
            },
            sent_telemetry,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Result;

    #[test]
    fn test_empty_payload() -> Result<()> {
        let empty = r#"{}"#;
        let _: CloseAllInactiveTabsPayload = serde_json::from_str(empty)?;
        Ok(())
    }

    #[test]
    fn test_payload() -> Result<()> {
        let (payload, telem) = CloseAllInactiveTabsPayload::new();
        let json = serde_json::to_string(&payload)?;
        assert!(!json.is_empty());
        assert_eq!(telem.flow_id.len(), 12);
        assert_eq!(telem.stream_id.len(), 12);
        assert_ne!(telem.flow_id, telem.stream_id);

        let deserialized: CloseAllInactiveTabsPayload = serde_json::from_str(&json)?;
        assert_eq!(deserialized, payload);

        Ok(())
    }
}
