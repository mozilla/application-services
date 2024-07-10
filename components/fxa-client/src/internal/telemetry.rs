/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{commands, FirefoxAccount};
use crate::Result;
use serde_derive::*;
use sync_guid::Guid;

impl FirefoxAccount {
    /// Gathers and resets telemetry for this account instance.
    /// This should be considered a short-term solution to telemetry gathering
    /// and should called whenever consumers expect there might be telemetry,
    /// and it should submit the telemetry to whatever telemetry system is in
    /// use (probably glean).
    ///
    /// The data is returned as a JSON string, which consumers should parse
    /// forgivingly (eg, be tolerant of things not existing) to try and avoid
    /// too many changes as telemetry comes and goes.
    pub fn gather_telemetry(&mut self) -> Result<String> {
        let telem = std::mem::replace(&mut self.telemetry, FxaTelemetry::new());
        Ok(serde_json::to_string(&telem)?)
    }
}

// A somewhat mixed-bag of all telemetry we want to collect. The idea is that
// the app will "pull" telemetry via a new API whenever it thinks there might
// be something to record.
// It's considered a temporary solution until either we can record it directly
// (eg, via glean) or we come up with something better.
// Note that this means we'll lose telemetry if we crash between gathering it
// here and the app submitting it, but that should be rare (in practice,
// apps will submit it directly after an operation that generated telememtry)

/// The reason a tab/command was received.
#[derive(Copy, Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReceivedReason {
    /// A push notification for the command was received.
    Push,
    /// Discovered while handling a push notification for a later message.
    PushMissed,
    /// Explicit polling for missed commands.
    Poll,
}

#[derive(Copy, Clone, Debug, Serialize)]
pub enum Command {
    #[serde(rename = "send_tab")]
    SendTab,
    #[serde(rename = "close_tabs")]
    CloseTabs,
}

#[derive(Debug, Serialize)]
pub struct SentCommand {
    pub command: Command,
    pub flow_id: String,
    pub stream_id: String,
}

impl SentCommand {
    pub fn for_send_tab() -> Self {
        Self::new(Command::SendTab)
    }

    pub fn for_close_tabs() -> Self {
        Self::new(Command::CloseTabs)
    }

    pub fn clone_with_new_stream_id(&self) -> Self {
        Self {
            command: self.command,
            flow_id: self.flow_id.clone(),
            stream_id: Guid::random().into_string(),
        }
    }

    fn new(command: Command) -> Self {
        Self {
            command,
            flow_id: Guid::random().into_string(),
            stream_id: Guid::random().into_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReceivedCommand {
    pub command: Command,
    pub flow_id: String,
    pub stream_id: String,
    pub reason: ReceivedReason,
}

impl ReceivedCommand {
    pub fn for_send_tab(payload: &commands::SendTabPayload, reason: ReceivedReason) -> Self {
        Self {
            command: Command::SendTab,
            flow_id: payload.flow_id.clone(),
            stream_id: payload.stream_id.clone(),
            reason,
        }
    }

    pub fn for_close_tabs(payload: &commands::CloseTabsPayload, reason: ReceivedReason) -> Self {
        Self {
            command: Command::SendTab,
            flow_id: payload.flow_id.clone(),
            stream_id: payload.stream_id.clone(),
            reason,
        }
    }
}

// We have a naive strategy to avoid unbounded memory growth - the intention
// is that if any platform lets things grow to hit these limits, it's probably
// never going to consume anything - so it doesn't matter what we discard (ie,
// there's no good reason to have a smarter circular buffer etc)
const MAX_TAB_EVENTS: usize = 200;

#[derive(Debug, Default, Serialize)]
pub struct FxaTelemetry {
    commands_sent: Vec<SentCommand>,
    commands_received: Vec<ReceivedCommand>,
}

impl FxaTelemetry {
    pub fn new() -> Self {
        FxaTelemetry {
            ..Default::default()
        }
    }

    pub fn record_command_sent(&mut self, sent: SentCommand) {
        if self.commands_sent.len() < MAX_TAB_EVENTS {
            self.commands_sent.push(sent);
        }
    }

    pub fn record_command_received(&mut self, recd: ReceivedCommand) {
        if self.commands_received.len() < MAX_TAB_EVENTS {
            self.commands_received.push(recd);
        }
    }
}
