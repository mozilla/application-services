/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// A somewhat mixed-bag of all telemetry we want to collect. The idea is that
// the app will "pull" telemetry via a new API whenever it thinks there might
// be something to record.
// It's considered a temporary solution until either we can record it directly
// (eg, via glean) or we come up with something better.

use serde_derive::*;

// First, constituent parts
#[derive(Debug, Serialize)]
pub struct SentReceivedTab {
    pub flow_id: String,
    pub stream_id: String,
}

// We have a naive strategy to avoid unbounded memory growth - the intention
// is that if any platform lets things grow to hit these limits, it's probably
// never going to consume anything - so it doesn't matter what we discard (ie,
// there's no good reason to have a smarter circular buffer etc)
const MAX_TAB_EVENTS: usize = 20;

#[derive(Debug, Serialize)]
pub struct FxaTelemetry {
    sent_tabs: Vec<SentReceivedTab>,
    received_tabs: Vec<SentReceivedTab>,
}

impl FxaTelemetry {
    pub fn new() -> Self {
        FxaTelemetry {
            sent_tabs: Vec::new(),
            received_tabs: Vec::new(),
        }
    }

    pub fn record_tab_sent(&mut self, sent: SentReceivedTab) {
        if self.sent_tabs.len() < MAX_TAB_EVENTS {
            self.sent_tabs.push(sent);
        }
    }

    pub fn record_tab_received(&mut self, recd: SentReceivedTab) {
        if self.received_tabs.len() < MAX_TAB_EVENTS {
            self.received_tabs.push(recd);
        }
    }
}
