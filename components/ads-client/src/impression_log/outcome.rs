/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[derive(Debug)]
pub enum ImpressionLogOutcome {
    // DB errors
    RecordImpressionFailed(rusqlite::Error), // Failed to record impression to log
    CountImpressionsFailed(rusqlite::Error), // Failed to get counts of impressions from log
    RetainImpressionsFailed(rusqlite::Error), // Failed to clear impressions from log
    // Simple events
    ImpressionCapHit,         // Impression limit reached for a cap_key
    ImpressionCapEnforced,    // Ad filtered from results due to CappingPolicy
    ImpressionCapNotEnforced, // Ad remained in results due to CappingPolicy
}
