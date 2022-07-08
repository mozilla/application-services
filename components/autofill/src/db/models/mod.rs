/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod address;
pub mod credit_card;
use types::Timestamp;

/// Metadata that's common between the records.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Default)]
pub struct Metadata {
    pub time_created: Timestamp,
    pub time_last_used: Timestamp,
    pub time_last_modified: Timestamp,
    pub times_used: i64,
    pub sync_change_counter: i64,
}
