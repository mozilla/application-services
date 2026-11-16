/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::glean_metrics;

pub enum KeyRegenerationEventReason {
    Lost,
    Corrupt,
    Other,
}

pub fn record_key_regeneration_event(reason: KeyRegenerationEventReason) {
    match reason {
        KeyRegenerationEventReason::Lost => {
            glean_metrics::logins_store::key_regenerated_lost.record(None);
        }
        KeyRegenerationEventReason::Corrupt => {
            glean_metrics::logins_store::key_regenerated_corrupt.record(None);
        }
        KeyRegenerationEventReason::Other => {
            glean_metrics::logins_store::key_regenerated_other.record(None);
        }
    }
}
