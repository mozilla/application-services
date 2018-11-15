/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod store;
pub mod record;
mod plan;

const MAX_INCOMING_PLACES: usize = 5000;
const MAX_OUTGOING_PLACES: usize = 5000;
const MAX_VISITS: usize = 20;
pub const HISTORY_TTL: u32 = 5184000; // 60 days in milliseconds
