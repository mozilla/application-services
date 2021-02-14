/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod address;
pub mod credit_card;
use serde::{Deserialize, Serialize};
use types::Timestamp;

/// Metadata that's common between the records.
#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Default)]
// Ideally we would not have `serde(default)` but some tests only supply
// partial json. I guess it doesn't matter much in practice.
#[serde(default)]
pub(crate) struct Metadata {
    // metadata isn't kebab-case for some reason...
    #[serde(rename = "timeCreated")]
    pub time_created: Timestamp,
    #[serde(rename = "timeLastUsed")]
    pub time_last_used: Timestamp,
    #[serde(rename = "timeLastModified")]
    pub time_last_modified: Timestamp,
    #[serde(rename = "timesUsed")]
    pub times_used: i64,
    // Version is always 1 - it's a field to make sync's life easy.
    //??    #[serde(default="1")]
    pub version: u32,
    // Change counter is never in json.
    #[serde(skip)]
    pub sync_change_counter: i64,
}
